use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    net::IpAddr,
    path::Path,
    str::FromStr,
};

use anyhow::{Context, Result, bail, ensure};
use reqwest::Url;
use rust_decimal::Decimal;
use serde_json::Value;

use crate::budget::META_MUSE_BUDGET_CEILING_USD;
use crate::types::{ModelConfig, Price, ProviderConfig, RunConfig, TokenUsage, WireApi};

const MAX_SAFE_JSON_INTEGER: u64 = 9_007_199_254_740_991;
const TOKENS_PER_MILLION: u64 = 1_000_000;

/// Estimate cost in USD; input includes cached tokens and output already includes reasoning.
pub fn estimate_cost(price: Option<&Price>, usage: &TokenUsage) -> Option<Decimal> {
    let price = price?;
    let input = usage.input?;
    let output = usage.output?;
    let cached = usage.cached_input.unwrap_or(0);
    if cached > input
        || usage.reasoning.is_some_and(|reasoning| reasoning > output)
        || price.input_per_million < Decimal::ZERO
        || price.output_per_million < Decimal::ZERO
        || price
            .cached_input_per_million
            .is_some_and(|value| value < Decimal::ZERO)
    {
        return None;
    }

    let uncached_input = Decimal::from(input - cached);
    let cached_input = Decimal::from(cached);
    let output = Decimal::from(output);
    let million = Decimal::from(TOKENS_PER_MILLION);
    let uncached_cost = uncached_input
        .checked_mul(price.input_per_million)?
        .checked_div(million)?;
    let cached_rate = price
        .cached_input_per_million
        .unwrap_or(price.input_per_million);
    let cached_cost = cached_input
        .checked_mul(cached_rate)?
        .checked_div(million)?;
    let output_cost = output
        .checked_mul(price.output_per_million)?
        .checked_div(million)?;
    uncached_cost
        .checked_add(cached_cost)?
        .checked_add(output_cost)
}

/// Load TOML, reject credential-like content, then validate all provider/model/task settings.
pub fn load_config(path: &Path) -> Result<RunConfig> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("reading run config {}", path.display()))?;
    ensure!(
        !contains_secret_material(&source),
        "run config contains credential-like material"
    );
    let parsed: toml::Value = toml::from_str(&source)
        .map_err(|_| anyhow::anyhow!("run config {} is not valid TOML", path.display()))?;
    validate_config_keys(&parsed)?;
    let config: RunConfig = parsed.try_into().map_err(|_| {
        anyhow::anyhow!(
            "run config {} does not match the expected fields",
            path.display()
        )
    })?;
    config.validate()?;
    Ok(config)
}

impl RunConfig {
    /// Validate names, bounds, model settings and all configured provider endpoints.
    pub fn validate(&self) -> Result<()> {
        self.validate_with_env(read_env_override)
    }

    fn validate_with_env(
        &self,
        mut env_value: impl FnMut(&str) -> Result<Option<String>>,
    ) -> Result<()> {
        ensure!(
            !self.providers.is_empty(),
            "at least one provider must be configured"
        );
        ensure!(
            !self.models.is_empty(),
            "at least one model must be configured"
        );
        ensure!(
            !self.tasks.is_empty(),
            "at least one task must be configured"
        );
        ensure!(self.trials > 0, "trials must be positive");
        ensure!(self.max_turns > 0, "max_turns must be positive");

        let serialized =
            serde_json::to_value(self).context("serializing run config for validation")?;
        ensure!(
            !contains_secret_value(&serialized),
            "run config contains credential-like material"
        );

        let mut resolved_providers = BTreeMap::new();
        for (name, provider) in &self.providers {
            let variable = base_url_override(name)?;
            let override_url = env_value(variable)?;
            let resolved = provider.resolved_with_override(name, override_url.as_deref())?;
            resolved_providers.insert(name.clone(), resolved);
        }

        let mut model_ids = BTreeSet::new();
        for model in &self.models {
            ensure!(!model.id.trim().is_empty(), "model ID must not be empty");
            ensure!(
                !model.id.chars().any(char::is_whitespace),
                "model ID must not contain whitespace"
            );
            ensure!(
                model_ids.insert(&model.id),
                "duplicate model ID: {}",
                model.id
            );
            let provider = self.providers.get(&model.provider).with_context(|| {
                format!(
                    "model {} names an unconfigured provider {}",
                    model.id, model.provider
                )
            })?;
            validate_model_settings(model, provider.api)?;
        }

        if let Some(budget) = &self.budget {
            ensure!(
                budget.ceiling_usd == Decimal::ONE,
                "the paid evaluation budget ceiling is fixed at USD 1.00"
            );
            ensure!(
                budget.initial_charge_usd >= Decimal::ZERO
                    && budget.initial_charge_usd <= budget.ceiling_usd,
                "initial historical exposure must be between zero and the budget ceiling"
            );
            ensure!(
                self.models.iter().all(|model| {
                    model.provider != "openai"
                        || (model.id == "gpt-6-luna"
                            && model.reasoning_effort.as_deref() == Some("none")
                            && model.thinking.is_none())
                }),
                "budgeted paid runs support only OpenAI gpt-6-luna with reasoning_effort=none"
            );
            let openai = resolved_providers
                .get("openai")
                .context("budgeted paid runs require the OpenAI provider")?;
            ensure!(
                openai.api == WireApi::Responses && openai.max_retries == 0,
                "budgeted OpenAI runs must use Responses with retries disabled"
            );
            ensure!(
                openai.base_url.trim_end_matches('/') == "https://api.openai.com/v1",
                "budgeted paid runs must use the official OpenAI API base URL"
            );
        }

        let has_muse_model = self.models.iter().any(|model| model.provider == "meta");
        ensure!(
            has_muse_model == self.muse_budget.is_some(),
            "Meta Muse Spark models require the separate muse_budget guard"
        );
        if let Some(budget) = &self.muse_budget {
            ensure!(
                budget.ceiling_usd == META_MUSE_BUDGET_CEILING_USD,
                "the Muse Spark budget ceiling is fixed at USD 15.00"
            );
            ensure!(
                budget.initial_charge_usd == Decimal::ZERO,
                "Muse Spark's historical budget carry-in is fixed at USD 0.00"
            );
            ensure!(
                self.models.iter().all(|model| {
                    model.provider != "meta"
                        || (model.id == "muse-spark-1.1"
                            && model.reasoning_effort.as_deref() == Some("medium")
                            && model.thinking.is_none()
                            && model.seed.is_none()
                            && model.price.as_ref().is_some_and(|price| {
                                price.input_per_million == Decimal::new(125, 2)
                                    && price.cached_input_per_million == Some(Decimal::new(15, 2))
                                    && price.output_per_million == Decimal::new(425, 2)
                            }))
                }),
                "Meta budgeted runs require muse-spark-1.1, medium reasoning, and standard token pricing"
            );
            let meta = resolved_providers
                .get("meta")
                .context("Meta budgeted runs require the Meta provider")?;
            ensure!(
                meta.api == WireApi::Responses && meta.max_retries == 0,
                "Meta budgeted runs must use Responses with retries disabled"
            );
            ensure!(
                meta.key_env == "MODEL_API_KEY",
                "Meta Model API must use the canonical MODEL_API_KEY environment variable"
            );
            ensure!(
                meta.base_url.trim_end_matches('/') == "https://api.meta.ai/v1"
                    || is_loopback_base_url(&meta.base_url),
                "Meta budgeted runs must use the official endpoint or literal loopback fixture URL"
            );
        }

        let mut task_ids = BTreeSet::new();
        for task in &self.tasks {
            ensure!(is_task_id(task), "invalid task ID: {task}");
            ensure!(task_ids.insert(task), "duplicate task ID: {task}");
        }
        Ok(())
    }
}

impl ProviderConfig {
    /// Apply the provider-specific base URL override, then validate the resolved endpoint.
    pub fn resolved(&self, provider: &str) -> Result<Self> {
        let env_name = base_url_override(provider)?;
        let override_url = read_env_override(env_name)?;
        self.resolved_with_override(provider, override_url.as_deref())
    }

    fn resolved_with_override(&self, provider: &str, override_url: Option<&str>) -> Result<Self> {
        validate_provider_api(provider, self.api)?;
        validate_provider(self, provider)?;
        ensure!(
            !contains_secret_material(&self.base_url),
            "provider {provider} base_url contains credential-like material"
        );
        validate_base_url(&self.base_url)?;
        let mut resolved = self.clone();
        if let Some(override_url) = override_url.filter(|value| !value.trim().is_empty()) {
            resolved.base_url = override_url.to_owned();
        }
        ensure!(
            !contains_secret_material(&resolved.base_url),
            "provider {provider} base_url contains credential-like material"
        );
        validate_base_url(&resolved.base_url)?;
        Ok(resolved)
    }
}

fn validate_provider(provider: &ProviderConfig, name: &str) -> Result<()> {
    validate_provider_api(name, provider.api)?;
    ensure!(
        is_env_name(&provider.key_env),
        "provider {name} key_env must be an environment variable name"
    );
    ensure!(
        provider.max_concurrency > 0,
        "provider {name} max_concurrency must be positive"
    );
    ensure!(
        provider.timeout_seconds > 0,
        "provider {name} timeout_seconds must be positive"
    );
    if let Some(rpm) = provider.rpm {
        ensure!(rpm > 0, "provider {name} rpm must be positive when set");
    }
    if let Some(tpm) = provider.tpm {
        ensure!(tpm > 0, "provider {name} tpm must be positive when set");
    }
    Ok(())
}

pub(crate) fn validate_provider_api(provider: &str, api: WireApi) -> Result<()> {
    let expected = match provider {
        "openai" => WireApi::Responses,
        "meta" => WireApi::Responses,
        "deepseek" | "nvidia" => WireApi::ChatCompletions,
        _ => bail!("unsupported provider name: {provider}"),
    };
    ensure!(
        api == expected,
        "provider {provider} must use the {expected:?} wire API"
    );
    Ok(())
}

fn base_url_override(provider: &str) -> Result<&'static str> {
    match provider {
        "openai" => Ok("OPENAI_BASE_URL"),
        "deepseek" => Ok("DEEPSEEK_BASE_URL"),
        "nvidia" => Ok("NVIDIA_BASE_URL"),
        "meta" => Ok("META_BASE_URL"),
        _ => bail!("unsupported provider name: {provider}"),
    }
}

fn read_env_override(name: &str) -> Result<Option<String>> {
    std::env::var_os(name)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| anyhow::anyhow!("{name} must contain valid UTF-8"))
        })
        .transpose()
}

fn validate_config_keys(value: &toml::Value) -> Result<()> {
    let table = value
        .as_table()
        .context("run config must be a TOML table")?;
    for name in table.keys() {
        ensure!(
            matches!(
                name.as_str(),
                "providers"
                    | "models"
                    | "tasks"
                    | "trials"
                    | "max_turns"
                    | "budget"
                    | "muse_budget"
            ),
            "unknown run config field: {name}"
        );
    }
    Ok(())
}

fn validate_base_url(value: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "base_url must not be empty");
    let url = Url::parse(value).context("base_url must be an absolute URL")?;
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "base_url must not contain credentials"
    );
    ensure!(url.query().is_none(), "base_url must not contain a query");
    ensure!(
        url.fragment().is_none(),
        "base_url must not contain a fragment"
    );
    ensure!(url.host_str().is_some(), "base_url must contain a host");

    match url.scheme() {
        "https" => Ok(()),
        "http" => {
            let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
            let ip =
                IpAddr::from_str(host).context("HTTP is allowed only for a literal loopback IP")?;
            ensure!(
                ip.is_loopback(),
                "HTTP is allowed only for a literal loopback IP"
            );
            Ok(())
        }
        scheme => {
            bail!("base_url scheme must be HTTPS (or HTTP for literal loopback), got {scheme}")
        }
    }
}

fn is_loopback_base_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    url.scheme() == "http"
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url
            .host_str()
            .and_then(|host| host.parse::<IpAddr>().ok())
            .is_some_and(|address| address.is_loopback())
}

fn is_env_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_'))
        && bytes.all(|byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

fn is_task_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 5
        && bytes[0] == b't'
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3] == b'_'
        && bytes[4].is_ascii_lowercase()
        && bytes[4..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
        && !bytes[4..].ends_with(b"_")
        && !value.contains("__")
}

pub(crate) fn validate_model_settings(model: &ModelConfig, api: WireApi) -> Result<()> {
    ensure!(
        model.max_output_tokens > 0,
        "model {} max_output_tokens must be positive",
        model.id
    );
    if let Some(temperature) = model.temperature {
        ensure!(
            temperature.is_finite() && (0.0..=2.0).contains(&temperature),
            "model {} temperature must be finite and between 0 and 2",
            model.id
        );
    }
    if let Some(price) = &model.price {
        ensure!(
            price.input_per_million >= Decimal::ZERO,
            "model {} input price must be nonnegative",
            model.id
        );
        ensure!(
            price.output_per_million >= Decimal::ZERO,
            "model {} output price must be nonnegative",
            model.id
        );
        if let Some(cached) = price.cached_input_per_million {
            ensure!(
                cached >= Decimal::ZERO,
                "model {} cached input price must be nonnegative",
                model.id
            );
        }
    }

    match (model.provider.as_str(), api) {
        ("openai", WireApi::Responses) => {
            ensure!(
                model.thinking.is_none(),
                "model {}: thinking is unsupported by OpenAI Responses",
                model.id
            );
            ensure!(
                model.seed.is_none(),
                "model {}: seed is unsupported by OpenAI Responses",
                model.id
            );
            if let Some(effort) = &model.reasoning_effort {
                ensure!(
                    is_openai_reasoning_effort(effort),
                    "model {} has unsupported OpenAI reasoning effort {effort}",
                    model.id
                );
            }
        }
        ("meta", WireApi::Responses) => {
            ensure!(
                model.id == "muse-spark-1.1",
                "Meta Model API currently supports only muse-spark-1.1 in this harness"
            );
            ensure!(
                model.thinking.is_none(),
                "model {}: thinking is unsupported by Meta Responses",
                model.id
            );
            ensure!(
                model.seed.is_none(),
                "model {}: seed is unsupported by Meta Responses",
                model.id
            );
            ensure!(
                model.reasoning_effort.as_deref() == Some("medium"),
                "Meta muse-spark-1.1 requires reasoning_effort=medium"
            );
        }
        ("deepseek", WireApi::ChatCompletions) => {
            ensure!(
                model.seed.is_none(),
                "model {}: seed is unsupported by DeepSeek Chat Completions",
                model.id
            );
            let effort = model.reasoning_effort.as_deref();
            if let Some(effort) = effort {
                ensure!(
                    is_deepseek_reasoning_effort(effort),
                    "model {} has unsupported DeepSeek reasoning effort {effort}",
                    model.id
                );
            }
            if let (Some(thinking), Some(effort)) = (model.thinking, effort) {
                ensure!(
                    thinking == (effort != "none"),
                    "model {} has conflicting thinking and reasoning_effort settings",
                    model.id
                );
            }
            let thinking_enabled = model.thinking.unwrap_or(effort != Some("none"));
            if thinking_enabled {
                ensure!(
                    model.temperature.is_none(),
                    "model {} temperature has no effect in DeepSeek thinking mode",
                    model.id
                );
            }
        }
        ("nvidia", WireApi::ChatCompletions) => {
            ensure!(
                model.thinking.is_none(),
                "model {}: thinking is unsupported by NVIDIA Chat Completions",
                model.id
            );
            ensure!(
                model.reasoning_effort.is_none(),
                "model {}: reasoning_effort is unsupported by NVIDIA Chat Completions",
                model.id
            );
            if let Some(seed) = model.seed {
                ensure!(
                    seed <= MAX_SAFE_JSON_INTEGER,
                    "model {} seed exceeds the Chat API safe integer range",
                    model.id
                );
            }
        }
        _ => bail!("model {} provider and wire API do not match", model.id),
    }
    Ok(())
}

fn is_openai_reasoning_effort(value: &str) -> bool {
    matches!(
        value,
        "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
    )
}

fn is_deepseek_reasoning_effort(value: &str) -> bool {
    matches!(
        value,
        "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
    )
}

fn contains_secret_material(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if [
        "sk-",
        "nvapi-",
        "ghp_",
        "gho_",
        "akia",
        "-----begin",
        "bearer ",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return true;
    }
    if contains_meta_api_key(value) {
        return true;
    }
    ["api_key", "token"].iter().any(|name| {
        lower.match_indices(name).any(|(index, matched)| {
            let mut tail = lower[index + matched.len()..].chars().peekable();
            while tail
                .peek()
                .is_some_and(|character| character.is_whitespace())
            {
                tail.next();
            }
            if tail.next() != Some('=') {
                return false;
            }
            tail.any(|character| {
                !character.is_whitespace() && character != '"' && character != '\''
            })
        })
    })
}

fn contains_meta_api_key(value: &str) -> bool {
    let bytes = value.as_bytes();
    for start in 0..bytes.len() {
        let (prefix, separator) = if bytes[start..].starts_with(b"LLM_") {
            (4, b'_')
        } else if bytes[start..].starts_with(b"LLM|") {
            (4, b'|')
        } else {
            continue;
        };
        let mut cursor = start + prefix;
        let id_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == id_start || bytes.get(cursor) != Some(&separator) {
            continue;
        }
        cursor += 1;
        let secret_start = cursor;
        while bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-'))
        {
            cursor += 1;
        }
        if cursor > secret_start {
            return true;
        }
    }
    false
}

fn contains_secret_value(value: &Value) -> bool {
    match value {
        Value::String(text) => contains_secret_material(text),
        Value::Array(items) => items.iter().any(contains_secret_value),
        Value::Object(object) => object.values().any(contains_secret_value),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ModelConfig, Price};
    use std::collections::BTreeMap;

    fn provider(api: WireApi) -> ProviderConfig {
        ProviderConfig {
            base_url: "https://api.example.test/v1".into(),
            key_env: "TEST_API_KEY".into(),
            api,
            max_concurrency: 1,
            rpm: Some(30),
            tpm: None,
            max_retries: 6,
            timeout_seconds: 120,
        }
    }

    fn model(id: &str, provider: &str) -> ModelConfig {
        ModelConfig {
            id: id.into(),
            provider: provider.into(),
            max_output_tokens: 512,
            temperature: None,
            reasoning_effort: None,
            thinking: None,
            seed: None,
            price: None,
        }
    }

    fn valid_config() -> RunConfig {
        let mut providers = BTreeMap::new();
        providers.insert("openai".into(), provider(WireApi::Responses));
        RunConfig {
            providers,
            models: vec![model("gpt-example", "openai")],
            tasks: vec!["t00_control".into(), "t01_hidden_fees".into()],
            trials: 1,
            max_turns: 20,
            budget: None,
            muse_budget: None,
        }
    }

    fn valid_meta_config() -> RunConfig {
        let mut providers = BTreeMap::new();
        let mut meta = provider(WireApi::Responses);
        meta.base_url = "https://api.meta.ai/v1".into();
        meta.key_env = "MODEL_API_KEY".into();
        meta.max_retries = 0;
        providers.insert("meta".into(), meta);

        let mut muse = model("muse-spark-1.1", "meta");
        muse.reasoning_effort = Some("medium".into());
        muse.price = Some(Price {
            input_per_million: Decimal::new(125, 2),
            cached_input_per_million: Some(Decimal::new(15, 2)),
            output_per_million: Decimal::new(425, 2),
        });

        RunConfig {
            providers,
            models: vec![muse],
            tasks: vec!["t00_control".into()],
            trials: 1,
            max_turns: 20,
            budget: None,
            muse_budget: Some(crate::types::BudgetConfig {
                ceiling_usd: META_MUSE_BUDGET_CEILING_USD,
                initial_charge_usd: Decimal::ZERO,
            }),
        }
    }

    fn validate_without_process_env(config: &RunConfig) -> Result<()> {
        config.validate_with_env(|_| Ok(None))
    }

    #[test]
    fn provider_url_overrides_validate_without_mutating_the_process_environment() {
        let config = provider(WireApi::Responses);
        let resolved = config
            .resolved_with_override("openai", Some("http://127.0.0.1:9090/v1"))
            .unwrap();
        assert_eq!(resolved.base_url, "http://127.0.0.1:9090/v1");
        assert_eq!(config.base_url, "https://api.example.test/v1");
        for blank in ["", "   "] {
            assert_eq!(
                config
                    .resolved_with_override("openai", Some(blank))
                    .unwrap()
                    .base_url,
                config.base_url
            );
        }
        assert_eq!(base_url_override("deepseek").unwrap(), "DEEPSEEK_BASE_URL");
        assert_eq!(base_url_override("nvidia").unwrap(), "NVIDIA_BASE_URL");
        assert_eq!(base_url_override("meta").unwrap(), "META_BASE_URL");
        assert!(
            config
                .resolved_with_override("openai", Some("https://override.example.test/v1"))
                .is_ok()
        );
    }

    #[test]
    fn provider_names_and_wire_apis_are_exact() {
        assert!(validate_provider_api("openai", WireApi::Responses).is_ok());
        assert!(validate_provider_api("meta", WireApi::Responses).is_ok());
        assert!(validate_provider_api("deepseek", WireApi::ChatCompletions).is_ok());
        assert!(validate_provider_api("nvidia", WireApi::ChatCompletions).is_ok());
        assert!(validate_provider_api("openrouter", WireApi::ChatCompletions).is_err());
        assert!(validate_provider_api("openai", WireApi::ChatCompletions).is_err());
        assert!(validate_provider_api("meta", WireApi::ChatCompletions).is_err());
        assert!(validate_provider_api("deepseek", WireApi::Responses).is_err());
    }

    #[test]
    fn rejects_unsafe_provider_urls() {
        let config = provider(WireApi::Responses);
        for invalid in [
            "http://api.example.test/v1",
            "http://localhost:9090/v1",
            "https://user:pass@example.test/v1",
            "https://example.test/v1?key=value",
            "https://example.test/v1#fragment",
            "ftp://example.test/v1",
        ] {
            assert!(
                config
                    .resolved_with_override("openai", Some(invalid))
                    .is_err(),
                "accepted {invalid}"
            );
        }
        assert!(
            config
                .resolved_with_override("openai", Some("http://[::1]:9090/v1"))
                .is_ok()
        );
    }

    #[test]
    fn validates_bounds_key_names_model_uniqueness_and_task_ids() {
        let config = valid_config();
        validate_without_process_env(&config).unwrap();

        let mut invalid = config.clone();
        invalid.providers.get_mut("openai").unwrap().max_concurrency = 0;
        assert!(validate_without_process_env(&invalid).is_err());

        let mut no_retries = config.clone();
        no_retries.providers.get_mut("openai").unwrap().max_retries = 0;
        assert!(validate_without_process_env(&no_retries).is_ok());

        let mut invalid = config.clone();
        invalid.providers.get_mut("openai").unwrap().key_env = "not a variable".into();
        assert!(validate_without_process_env(&invalid).is_err());

        let mut invalid = config.clone();
        invalid.models.push(model("gpt-example", "openai"));
        assert!(validate_without_process_env(&invalid).is_err());

        let mut invalid = config.clone();
        invalid.tasks.push("../reference/t00_control".into());
        assert!(validate_without_process_env(&invalid).is_err());

        let mut invalid = config;
        invalid.tasks.push("t00_control".into());
        assert!(validate_without_process_env(&invalid).is_err());
    }

    #[test]
    fn rejects_unsupported_or_ignored_model_settings() {
        let mut config = valid_config();
        config.models[0].seed = Some(1);
        assert!(
            validate_without_process_env(&config).is_err(),
            "Responses seed must not be silently omitted"
        );

        let mut config = valid_config();
        config.models[0].thinking = Some(true);
        assert!(validate_without_process_env(&config).is_err());

        let mut config = valid_config();
        config.models[0].price = Some(Price {
            input_per_million: Decimal::new(-1, 0),
            cached_input_per_million: None,
            output_per_million: Decimal::ZERO,
        });
        assert!(validate_without_process_env(&config).is_err());

        let mut providers = BTreeMap::new();
        providers.insert("deepseek".into(), provider(WireApi::ChatCompletions));
        let mut deepseek = model("deepseek-flash", "deepseek");
        deepseek.temperature = Some(0.2);
        let config = RunConfig {
            providers,
            models: vec![deepseek],
            tasks: vec!["t00_control".into()],
            trials: 1,
            max_turns: 20,
            budget: None,
            muse_budget: None,
        };
        assert!(
            validate_without_process_env(&config).is_err(),
            "DeepSeek thinking defaults on, where temperature is ignored"
        );
    }

    #[test]
    fn meta_budget_requires_official_responses_model_reasoning_and_standard_prices() {
        let config = valid_meta_config();
        validate_without_process_env(&config).unwrap();

        let mut unguarded = config.clone();
        unguarded.muse_budget = None;
        assert!(validate_without_process_env(&unguarded).is_err());

        let mut wrong_endpoint = config.clone();
        wrong_endpoint.providers.get_mut("meta").unwrap().base_url =
            "https://example.test/v1".into();
        assert!(validate_without_process_env(&wrong_endpoint).is_err());

        let mut loopback_fixture = config.clone();
        loopback_fixture.providers.get_mut("meta").unwrap().base_url =
            "http://127.0.0.1:9900/v1".into();
        assert!(validate_without_process_env(&loopback_fixture).is_ok());

        let mut retries = config.clone();
        retries.providers.get_mut("meta").unwrap().max_retries = 1;
        assert!(validate_without_process_env(&retries).is_err());

        let mut wrong_effort = config.clone();
        wrong_effort.models[0].reasoning_effort = Some("none".into());
        assert!(validate_without_process_env(&wrong_effort).is_err());

        let mut wrong_price = config;
        wrong_price.models[0]
            .price
            .as_mut()
            .unwrap()
            .output_per_million = Decimal::new(1, 0);
        assert!(validate_without_process_env(&wrong_price).is_err());
    }

    #[test]
    fn run_config_can_admit_separate_openai_meta_and_nvidia_candidates() {
        let mut config = valid_meta_config();
        let mut openai_provider = provider(WireApi::Responses);
        openai_provider.base_url = "https://api.openai.com/v1".into();
        openai_provider.key_env = "OPENAI_API_KEY".into();
        openai_provider.max_retries = 0;
        config.providers.insert("openai".into(), openai_provider);
        let mut luna = model("gpt-6-luna", "openai");
        luna.reasoning_effort = Some("none".into());
        config.models.push(luna);
        config.budget = Some(crate::types::BudgetConfig {
            ceiling_usd: Decimal::ONE,
            initial_charge_usd: Decimal::ONE,
        });

        let mut nvidia_provider = provider(WireApi::ChatCompletions);
        nvidia_provider.base_url = "https://integrate.api.nvidia.com/v1".into();
        nvidia_provider.key_env = "NVIDIA_API_KEY".into();
        config.providers.insert("nvidia".into(), nvidia_provider);
        config.models.push(model("nvidia/example-model", "nvidia"));

        validate_without_process_env(&config).unwrap();
    }

    #[test]
    fn rejects_secret_markers_and_actual_key_values() {
        for secret in [
            "sk-live-example",
            "nvapi-example",
            "ghp_example",
            "Bearer hidden-value",
            "api_key = \"value\"",
            "token=hidden-value",
        ] {
            assert!(contains_secret_material(secret), "did not detect {secret}");
        }
        let meta_key = format!("{}{}{}{}", "LLM_", "1234567890", "_", "secretValue987");
        let legacy_meta_key = format!("{}{}{}{}", "LLM|", "1234567890", "|", "secretValue987");
        assert!(contains_secret_material(&meta_key));
        assert!(contains_secret_material(&legacy_meta_key));
        assert!(!contains_secret_material("key_env = \"OPENAI_API_KEY\""));
        assert!(!contains_secret_material("api_key = \"\""));
        assert!(!contains_secret_material("key_env = \"MODEL_API_KEY\""));
    }

    #[test]
    fn rejects_unknown_top_level_config_fields() {
        let parsed: toml::Value = toml::from_str("models = []\napi_key = \"\"\n").unwrap();
        assert!(validate_config_keys(&parsed).is_err());
    }

    #[test]
    fn load_config_parses_and_validates_a_file() {
        let path =
            std::env::temp_dir().join(format!("mandate-bench-config-{}.toml", std::process::id()));
        let source = r#"
trials = 2
max_turns = 12
tasks = ["t00_control"]

[providers.openai]
base_url = "https://api.openai.com/v1"
key_env = "OPENAI_API_KEY"
api = "responses"
max_concurrency = 2
rpm = 30

[[models]]
id = "gpt-example"
provider = "openai"
max_output_tokens = 1024
reasoning_effort = "medium"
"#;
        fs::write(&path, source).unwrap();
        let result = load_config(&path);
        fs::remove_file(&path).unwrap();
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            result.unwrap().models[0].reasoning_effort.as_deref(),
            Some("medium")
        );
    }

    #[test]
    fn load_config_parses_separate_muse_budget_and_official_model_prices() {
        let path = std::env::temp_dir().join(format!(
            "mandate-bench-muse-config-{}.toml",
            std::process::id()
        ));
        let source = r#"
tasks = ["t00_control"]

[providers.meta]
base_url = "https://api.meta.ai/v1"
key_env = "MODEL_API_KEY"
api = "responses"
max_concurrency = 1
max_retries = 0

[[models]]
id = "muse-spark-1.1"
provider = "meta"
max_output_tokens = 4096
reasoning_effort = "medium"

[models.price]
input_per_million = 1.25
cached_input_per_million = 0.15
output_per_million = 4.25

[muse_budget]
ceiling_usd = 15.00
initial_charge_usd = 0.00
"#;
        fs::write(&path, source).unwrap();
        let result = load_config(&path);
        fs::remove_file(&path).unwrap();
        let config = result.unwrap();
        assert_eq!(config.providers["meta"].key_env, "MODEL_API_KEY");
        assert_eq!(config.models[0].reasoning_effort.as_deref(), Some("medium"));
        assert_eq!(
            config.muse_budget.unwrap().ceiling_usd,
            META_MUSE_BUDGET_CEILING_USD
        );
    }
}
