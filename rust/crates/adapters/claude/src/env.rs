use std::collections::{BTreeMap, BTreeSet};

const ENV_OVERRIDES: &[(&str, &str)] = &[
    ("CLAUDE_CODE_ENABLE_TELEMETRY", ""),
    ("DO_NOT_TRACK", "1"),
    ("OTEL_SDK_DISABLED", "true"),
    ("OTEL_TRACES_EXPORTER", "none"),
    ("OTEL_METRICS_EXPORTER", "none"),
    ("OTEL_LOGS_EXPORTER", "none"),
    ("SENTRY_DSN", ""),
    ("DISABLE_ERROR_REPORTING", "1"),
    ("DISABLE_BUG_COMMAND", "1"),
    ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
    ("TELEMETRY_DISABLED", "1"),
    ("DISABLE_TELEMETRY", "1"),
];

const ENV_UNSETS: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
];

pub fn claude_env_overrides() -> BTreeMap<String, String> {
    ENV_OVERRIDES
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

pub fn claude_env_unsets() -> BTreeSet<String> {
    ENV_UNSETS.iter().map(|key| (*key).to_string()).collect()
}
