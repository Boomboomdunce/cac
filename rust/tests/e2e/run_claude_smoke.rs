use assert_cmd::Command;
use serde_json::Value;
use std::path::PathBuf;

fn fixture_fake_claude_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fake_claude.js")
}

#[test]
fn run_claude_injects_expected_environment() {
    let temp = tempfile::tempdir().unwrap();
    let fake = fixture_fake_claude_path();
    let state_root = temp.path().join("state root with spaces");

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    let mut command = Command::cargo_bin("ccp").unwrap();
    command
        .env("CCP_STATE_ROOT", &state_root)
        .env("ANTHROPIC_BASE_URL", "https://example.invalid")
        .env("ANTHROPIC_AUTH_TOKEN", "test-auth-token")
        .env("ANTHROPIC_API_KEY", "test-api-key")
        .args(["run", "--profile", "work", "--"]);
    #[cfg(unix)]
    command.arg(&fake);
    #[cfg(windows)]
    command.args(["node"]).arg(&fake);

    let output = command.output().unwrap();

    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();

    assert!(payload
        .get("CCP_SESSION_ID")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty()));
    assert!(payload
        .get("CCP_RUNTIME_HOOK")
        .and_then(Value::as_str)
        .is_some_and(|value| value.ends_with(".js")));
    assert!(payload
        .get("NODE_OPTIONS")
        .and_then(Value::as_str)
        .is_some_and(|value| value.contains("--require")));
    assert_eq!(
        payload.get("ANTHROPIC_BASE_URL").and_then(Value::as_str),
        None
    );
    assert_eq!(
        payload.get("ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str),
        None
    );
    assert_eq!(
        payload.get("ANTHROPIC_API_KEY").and_then(Value::as_str),
        None
    );
    assert_eq!(
        payload.get("DO_NOT_TRACK").and_then(Value::as_str),
        Some("1")
    );
    assert_eq!(
        payload.get("OTEL_SDK_DISABLED").and_then(Value::as_str),
        Some("true")
    );
    assert_eq!(
        payload
            .get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")
            .and_then(Value::as_str),
        Some("1")
    );
    assert_eq!(
        payload.get("DISABLE_TELEMETRY").and_then(Value::as_str),
        Some("1")
    );
}
