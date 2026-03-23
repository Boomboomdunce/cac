use claude_adapter::claude_adapter;

#[test]
fn policy_includes_blocked_telemetry_hosts() {
    let adapter = claude_adapter();
    let blocked_hosts = adapter.blocked_hosts();

    assert!(blocked_hosts.contains("statsig.anthropic.com"));
    assert!(blocked_hosts.contains("sentry.io"));
    assert!(blocked_hosts.contains("o1137031.ingest.sentry.io"));
}

#[test]
fn policy_requires_node_preload_runtime_hook() {
    let adapter = claude_adapter();

    assert!(adapter.required_capabilities().contains("node_preload"));
    let hook = adapter.runtime_hook_bundle();
    assert_eq!(hook.relative_path(), "hooks/node/claude-preload.js");
    assert!(hook.contents().contains("dns.lookup"));
    assert!(hook.contents().contains("fetch"));
}

#[test]
fn policy_marks_sidecar_as_required() {
    let adapter = claude_adapter();

    assert!(adapter.sidecar_required());
    assert!(adapter.required_capabilities().contains("sidecar"));
}

#[test]
fn policy_includes_claude_specific_telemetry_toggles() {
    let adapter = claude_adapter();
    let env = adapter.environment_overrides();

    assert_eq!(env.get("DO_NOT_TRACK").map(String::as_str), Some("1"));
    assert_eq!(
        env.get("OTEL_SDK_DISABLED").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        env.get("OTEL_TRACES_EXPORTER").map(String::as_str),
        Some("none")
    );
    assert_eq!(
        env.get("OTEL_METRICS_EXPORTER").map(String::as_str),
        Some("none")
    );
    assert_eq!(
        env.get("OTEL_LOGS_EXPORTER").map(String::as_str),
        Some("none")
    );
    assert_eq!(env.get("SENTRY_DSN").map(String::as_str), Some(""));
    assert_eq!(
        env.get("DISABLE_ERROR_REPORTING").map(String::as_str),
        Some("1")
    );
    assert_eq!(env.get("DISABLE_BUG_COMMAND").map(String::as_str), Some("1"));
    assert_eq!(
        env.get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")
            .map(String::as_str),
        Some("1")
    );
    assert_eq!(
        env.get("TELEMETRY_DISABLED").map(String::as_str),
        Some("1")
    );
    assert_eq!(env.get("DISABLE_TELEMETRY").map(String::as_str), Some("1"));
    assert_eq!(
        env.get("CLAUDE_CODE_ENABLE_TELEMETRY").map(String::as_str),
        Some("")
    );
}
