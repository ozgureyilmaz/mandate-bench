//! Shared per-provider request scheduling and concurrency limits.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, Weak},
};

use anyhow::{Result, bail};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time::{Duration, Instant, sleep_until},
};

use crate::types::ProviderConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Settings {
    max_concurrency: usize,
    rpm: Option<u32>,
    tpm: Option<u64>,
}

/// Rate and concurrency state shared by every client created for one provider.
pub struct ProviderLimits {
    settings: Settings,
    semaphore: Arc<Semaphore>,
    schedule: tokio::sync::Mutex<Schedule>,
}

#[derive(Default)]
struct Schedule {
    next_rpm: Option<Instant>,
    next_tpm: Option<Instant>,
}

static REGISTRY: OnceLock<Mutex<HashMap<String, Weak<ProviderLimits>>>> = OnceLock::new();

impl ProviderLimits {
    fn new(settings: Settings) -> Self {
        Self {
            settings,
            semaphore: Arc::new(Semaphore::new(settings.max_concurrency)),
            schedule: tokio::sync::Mutex::new(Schedule::default()),
        }
    }

    pub(crate) fn for_provider(name: &str, config: &ProviderConfig) -> Result<Arc<Self>> {
        let settings = Settings {
            max_concurrency: config.max_concurrency,
            rpm: config.rpm,
            tpm: config.tpm,
        };
        if settings.max_concurrency == 0 {
            bail!("provider concurrency limit must be positive");
        }
        if settings.rpm == Some(0) || settings.tpm == Some(0) {
            bail!("provider rate limits must be positive when configured");
        }

        let registry = REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
        let mut registry = registry
            .lock()
            .map_err(|_| anyhow::anyhow!("provider limit registry is unavailable"))?;
        if let Some(existing) = registry.get(name).and_then(Weak::upgrade) {
            if existing.settings != settings {
                bail!("provider limit settings changed while clients are active");
            }
            return Ok(existing);
        }
        registry.retain(|_, value| value.strong_count() > 0);
        let limits = Arc::new(Self::new(settings));
        registry.insert(name.to_owned(), Arc::downgrade(&limits));
        Ok(limits)
    }

    /// Take a provider concurrency slot before assigning its rate slot. This
    /// prevents callers queued behind slow requests from accumulating stale
    /// reservations and then sending together when a permit becomes available.
    pub(crate) async fn begin_attempt(
        &self,
        estimated_tokens: u64,
    ) -> Result<OwnedSemaphorePermit> {
        self.validate_reservation(estimated_tokens)?;
        let permit = Arc::clone(&self.semaphore)
            .acquire_owned()
            .await
            .map_err(|_| anyhow::anyhow!("provider concurrency limit is closed"))?;
        self.reserve_rate_slot(estimated_tokens).await;
        Ok(permit)
    }

    pub(crate) fn validate_reservation(&self, estimated_tokens: u64) -> Result<()> {
        if let Some(capacity) = self.settings.tpm
            && estimated_tokens > capacity
        {
            bail!("request token reservation exceeds provider TPM capacity");
        }
        Ok(())
    }

    /// Reserve this attempt in the provider's no-burst RPM/TPM schedules.
    /// Called only after acquiring a semaphore permit.
    async fn reserve_rate_slot(&self, estimated_tokens: u64) {
        let start_at = {
            let mut schedule = self.schedule.lock().await;
            let now = Instant::now();
            let mut start = now;
            if let Some(next) = schedule.next_rpm.filter(|next| *next > start) {
                start = next;
            }
            if let Some(next) = schedule.next_tpm.filter(|next| *next > start) {
                start = next;
            }

            if let Some(rpm) = self.settings.rpm {
                schedule.next_rpm = Some(start + rate_interval(1, u64::from(rpm)));
            }
            if let Some(tpm) = self.settings.tpm {
                schedule.next_tpm = Some(start + rate_interval(estimated_tokens, tpm));
            }
            start
        };
        sleep_until(start_at).await;
    }
}

fn rate_interval(units: u64, per_minute: u64) -> Duration {
    let nanos_per_minute = 60_000_000_000_u128;
    let numerator = u128::from(units).saturating_mul(nanos_per_minute);
    let rounded =
        numerator.saturating_add(u128::from(per_minute).saturating_sub(1)) / u128::from(per_minute);
    Duration::from_nanos(rounded.min(u128::from(u64::MAX)) as u64)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::{Instant, advance};

    use super::{ProviderLimits, Settings};
    use crate::types::{ProviderConfig, WireApi};

    #[tokio::test(start_paused = true)]
    async fn rpm_reservations_are_shared_and_have_no_burst() {
        let limits = std::sync::Arc::new(ProviderLimits::new(Settings {
            max_concurrency: 2,
            rpm: Some(60),
            tpm: None,
        }));
        let start = Instant::now();
        let first = limits.begin_attempt(100).await.unwrap();
        assert_eq!(Instant::now() - start, Duration::ZERO);
        drop(first);

        let other_model_client = std::sync::Arc::clone(&limits);
        let second = other_model_client.begin_attempt(100).await.unwrap();
        assert_eq!(Instant::now() - start, Duration::from_secs(1));
        drop(second);
    }

    #[tokio::test(start_paused = true)]
    async fn clients_for_one_provider_share_the_same_buckets_across_models() {
        let config = ProviderConfig {
            base_url: "https://api.example.test/v1".into(),
            key_env: "TEST_API_KEY".into(),
            api: WireApi::ChatCompletions,
            max_concurrency: 2,
            rpm: Some(60),
            tpm: Some(120),
            max_retries: 1,
            timeout_seconds: 1,
        };
        let first_model_client =
            ProviderLimits::for_provider("shared-test-nvidia", &config).unwrap();
        let second_model_client =
            ProviderLimits::for_provider("shared-test-nvidia", &config).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &first_model_client,
            &second_model_client
        ));

        let start = Instant::now();
        let first = first_model_client.begin_attempt(30).await.unwrap();
        drop(first);
        let second = second_model_client.begin_attempt(30).await.unwrap();
        assert_eq!(Instant::now() - start, Duration::from_secs(15));
        assert_eq!(
            second_model_client.schedule.lock().await.next_rpm.unwrap() - Instant::now(),
            Duration::from_secs(1)
        );
        assert_eq!(
            second_model_client.schedule.lock().await.next_tpm.unwrap() - Instant::now(),
            Duration::from_secs(15)
        );
        drop(second);
    }

    #[tokio::test]
    async fn provider_semaphore_is_shared_and_released_after_each_request() {
        let config = ProviderConfig {
            base_url: "https://api.example.test/v1".into(),
            key_env: "TEST_API_KEY".into(),
            api: WireApi::ChatCompletions,
            max_concurrency: 2,
            rpm: None,
            tpm: None,
            max_retries: 1,
            timeout_seconds: 1,
        };
        let first_model_client =
            ProviderLimits::for_provider("shared-semaphore-test", &config).unwrap();
        let second_model_client =
            ProviderLimits::for_provider("shared-semaphore-test", &config).unwrap();
        let first = first_model_client.begin_attempt(1).await.unwrap();
        let second = second_model_client.begin_attempt(1).await.unwrap();
        assert_eq!(first_model_client.semaphore.available_permits(), 0);

        drop(first);
        let third = second_model_client.begin_attempt(1).await.unwrap();
        assert_eq!(first_model_client.semaphore.available_permits(), 0);
        drop((second, third));
        assert_eq!(first_model_client.semaphore.available_permits(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn tpm_reservations_are_weighted_and_capacity_is_hard() {
        let limits = ProviderLimits::new(Settings {
            max_concurrency: 1,
            rpm: None,
            tpm: Some(60),
        });
        let start = Instant::now();
        let first = limits.begin_attempt(15).await.unwrap();
        drop(first);
        advance(Duration::from_secs(14)).await;
        assert_eq!(Instant::now() - start, Duration::from_secs(14));
        let second = limits.begin_attempt(15).await.unwrap();
        assert_eq!(Instant::now() - start, Duration::from_secs(15));
        drop(second);
        assert!(limits.validate_reservation(61).is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn queued_calls_do_not_reserve_rate_slots_before_the_semaphore() {
        let limits = std::sync::Arc::new(ProviderLimits::new(Settings {
            max_concurrency: 1,
            rpm: Some(60),
            tpm: None,
        }));
        let start = Instant::now();
        let first_permit = limits.begin_attempt(1).await.unwrap();

        let mut queued = Vec::new();
        for _ in 0..2 {
            let limits = std::sync::Arc::clone(&limits);
            queued.push(tokio::spawn(async move {
                let permit = limits.begin_attempt(1).await.unwrap();
                let sent_at = Instant::now() - start;
                drop(permit);
                sent_at
            }));
        }
        tokio::task::yield_now().await;
        advance(Duration::from_secs(5)).await;
        drop(first_permit);

        let mut sent_times = Vec::new();
        for request in queued {
            sent_times.push(request.await.unwrap());
        }
        sent_times.sort();
        assert_eq!(sent_times, [Duration::from_secs(5), Duration::from_secs(6)]);
    }
}
