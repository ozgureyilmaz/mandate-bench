//! Retry classification and provider-directed backoff delays.

use std::time::{Duration, SystemTime};

use rand::Rng;
use reqwest::{StatusCode, header::HeaderMap};

pub(crate) fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
}

/// Honor the longest usable provider delay; otherwise use capped full jitter.
/// `failure_number` starts at 1 for the first failed attempt.
pub(crate) fn retry_delay(headers: &HeaderMap, failure_number: u32) -> Duration {
    let now = SystemTime::now();
    let mut server_delays = Vec::new();
    for name in [
        "retry-after",
        "x-ratelimit-reset-requests",
        "x-ratelimit-reset-tokens",
    ] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok())
            && let Some(delay) = parse_header_delay(name, value.trim(), now)
        {
            server_delays.push(delay);
        }
    }
    if let Some(delay) = server_delays.into_iter().max() {
        return delay;
    }

    let exponent = failure_number.saturating_sub(1).min(5);
    let cap_seconds = (2_u64 << exponent).min(60);
    let cap_millis = cap_seconds * 1_000;
    let mut rng = rand::rng();
    Duration::from_millis(rng.random_range(0..=cap_millis))
}

fn parse_header_delay(name: &str, value: &str, now: SystemTime) -> Option<Duration> {
    if name == "retry-after"
        && let Ok(seconds) = value.parse::<f64>()
    {
        return nonnegative_seconds(seconds);
    }
    if let Some(duration) = parse_duration(value) {
        return Some(duration);
    }
    let date = httpdate::parse_http_date(value).ok()?;
    date.duration_since(now).ok()
}

fn nonnegative_seconds(seconds: f64) -> Option<Duration> {
    if !seconds.is_finite() || seconds < 0.0 || seconds >= u64::MAX as f64 {
        return None;
    }
    let whole_seconds = seconds.floor() as u64;
    let subsecond_nanos = ((seconds - whole_seconds as f64) * 1_000_000_000.0).ceil() as u32;
    Some(Duration::new(
        whole_seconds,
        subsecond_nanos.min(999_999_999),
    ))
}

/// Parse the duration forms used by rate-limit reset headers (`1s`, `2m3s`, `500ms`).
fn parse_duration(value: &str) -> Option<Duration> {
    if let Ok(seconds) = value.parse::<f64>() {
        return nonnegative_seconds(seconds);
    }

    let bytes = value.as_bytes();
    let mut index = 0;
    let mut total_nanos = 0_u128;
    let mut parsed = false;
    while index < bytes.len() {
        let number_start = index;
        while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'.') {
            index += 1;
        }
        if number_start == index {
            return None;
        }
        let number = value.get(number_start..index)?.parse::<f64>().ok()?;
        if !number.is_finite() || number < 0.0 {
            return None;
        }
        let unit_start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        let unit = value.get(unit_start..index)?;
        let nanos_per_unit = match unit {
            "ns" => 1_u128,
            "us" => 1_000,
            "ms" => 1_000_000,
            "s" => 1_000_000_000,
            "m" => 60_000_000_000,
            "h" => 3_600_000_000_000,
            _ => return None,
        };
        let part = (number * nanos_per_unit as f64).ceil();
        if !part.is_finite() || part < 0.0 || part > u128::MAX as f64 {
            return None;
        }
        total_nanos = total_nanos.saturating_add(part as u128);
        parsed = true;
    }
    if !parsed {
        return None;
    }
    let seconds = total_nanos / 1_000_000_000;
    let nanos = (total_nanos % 1_000_000_000) as u32;
    Some(Duration::new(
        seconds.min(u128::from(u64::MAX)) as u64,
        nanos,
    ))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use reqwest::header::{HeaderMap, HeaderValue};

    use super::{is_retryable_status, retry_delay};

    #[test]
    fn status_classification_retries_only_the_contract_statuses() {
        assert!(is_retryable_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(reqwest::StatusCode::REQUEST_TIMEOUT));
        assert!(is_retryable_status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(!is_retryable_status(reqwest::StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(reqwest::StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn server_delay_uses_the_maximum_of_retry_after_and_both_reset_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("2"));
        headers.insert("x-ratelimit-reset-requests", HeaderValue::from_static("4s"));
        headers.insert("x-ratelimit-reset-tokens", HeaderValue::from_static("1m3s"));
        assert_eq!(retry_delay(&headers, 1), Duration::from_secs(63));
    }

    #[test]
    fn reset_headers_accept_http_dates() {
        let mut headers = HeaderMap::new();
        let future =
            httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(10));
        headers.insert("retry-after", HeaderValue::from_str(&future).unwrap());
        let delay = retry_delay(&headers, 1);
        assert!(delay >= Duration::from_secs(8));
        assert!(delay <= Duration::from_secs(10));
    }

    #[test]
    fn absent_headers_use_bounded_full_jitter() {
        let headers = HeaderMap::new();
        assert!(retry_delay(&headers, 1) <= Duration::from_secs(2));
        assert!(retry_delay(&headers, 100) <= Duration::from_secs(60));
    }
}
