use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

fn fixture_fake_claude_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fake_claude.js")
}

fn reachable_proxy_url() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        for _ in 0..4 {
            if listener.accept().is_err() {
                break;
            }
        }
    });
    (format!("http://{}", address), handle)
}

#[test]
fn run_claude_injects_expected_environment() {
    let temp = tempfile::tempdir().unwrap();
    let fake = fixture_fake_claude_path();
    let state_root = temp.path().join("state root with spaces");
    let (proxy_url, _proxy_thread) = reachable_proxy_url();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .args([
            "profile",
            "create",
            "work",
            "--adapter",
            "claude",
            "--proxy",
            &proxy_url,
        ])
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
    let identity_root = state_root.join("identities/work");
    let expected_hostname = fs::read_to_string(identity_root.join("hostname"))
        .unwrap()
        .trim()
        .to_string();
    let expected_machine_id = fs::read_to_string(identity_root.join("machine_id"))
        .unwrap()
        .trim()
        .to_string();
    let expected_tz = fs::read_to_string(identity_root.join("tz"))
        .unwrap()
        .trim()
        .to_string();
    let expected_lang = fs::read_to_string(identity_root.join("lang"))
        .unwrap()
        .trim()
        .to_string();
    #[cfg(any(target_os = "macos", windows))]
    let expected_uuid = fs::read_to_string(identity_root.join("uuid"))
        .unwrap()
        .trim()
        .to_string();

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
    assert_eq!(
        payload.get("HTTPS_PROXY").and_then(Value::as_str),
        Some(proxy_url.as_str())
    );
    assert_eq!(
        payload.get("HTTP_PROXY").and_then(Value::as_str),
        Some(proxy_url.as_str())
    );
    assert_eq!(
        payload.get("ALL_PROXY").and_then(Value::as_str),
        Some(proxy_url.as_str())
    );
    assert_eq!(
        payload.get("NO_PROXY").and_then(Value::as_str),
        Some("localhost,127.0.0.1")
    );
    assert!(payload
        .get("CCP_MTLS_CERT")
        .and_then(Value::as_str)
        .is_some_and(|value| value.ends_with("certs/work/client_cert.pem")));
    assert!(payload
        .get("CCP_MTLS_KEY")
        .and_then(Value::as_str)
        .is_some_and(|value| value.ends_with("certs/work/client_key.pem")));
    assert!(payload
        .get("CCP_MTLS_CA")
        .and_then(Value::as_str)
        .is_some_and(|value| value.ends_with("certs/ca/ca_cert.pem")));
    assert_eq!(
        payload.get("CAC_MTLS_CERT").and_then(Value::as_str),
        payload.get("CCP_MTLS_CERT").and_then(Value::as_str)
    );
    assert_eq!(
        payload.get("CAC_MTLS_KEY").and_then(Value::as_str),
        payload.get("CCP_MTLS_KEY").and_then(Value::as_str)
    );
    assert_eq!(
        payload.get("CAC_MTLS_CA").and_then(Value::as_str),
        payload.get("CCP_MTLS_CA").and_then(Value::as_str)
    );
    assert_eq!(
        payload.get("NODE_EXTRA_CA_CERTS").and_then(Value::as_str),
        payload.get("CCP_MTLS_CA").and_then(Value::as_str)
    );
    assert!(payload
        .get("HOSTALIASES")
        .and_then(Value::as_str)
        .is_some_and(|value| value.ends_with("config/blocked_hosts")));
    let blocked_hosts = fs::read_to_string(state_root.join("config/blocked_hosts")).unwrap();
    assert!(blocked_hosts.contains("statsig.anthropic.com"));
    assert!(blocked_hosts.contains("sentry.io"));
    assert_eq!(
        payload.get("CCP_PROXY_HOST").and_then(Value::as_str),
        Some(proxy_url.trim_start_matches("http://"))
    );
    assert_eq!(
        payload.get("CAC_PROXY_HOST").and_then(Value::as_str),
        Some(proxy_url.trim_start_matches("http://"))
    );
    assert_eq!(
        payload.get("HOSTNAME").and_then(Value::as_str),
        Some(expected_hostname.as_str())
    );
    assert_eq!(
        payload.get("COMPUTERNAME").and_then(Value::as_str),
        Some(expected_hostname.as_str())
    );
    assert_eq!(
        payload.get("TZ").and_then(Value::as_str),
        Some(expected_tz.as_str())
    );
    assert_eq!(
        payload.get("LANG").and_then(Value::as_str),
        Some(expected_lang.as_str())
    );
    assert_eq!(
        payload.get("hostnameCommand").and_then(Value::as_str),
        Some(expected_hostname.as_str())
    );
    assert_eq!(
        payload.get("machineIdCommand").and_then(Value::as_str),
        Some(expected_machine_id.as_str())
    );

    #[cfg(windows)]
    {
        let expected_windows_mac =
            expected_mac_address(&fs::read_to_string(identity_root.join("mac_address")).unwrap());
        assert!(payload
            .get("platformUuidCommand")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains(expected_uuid.as_str())));
        assert!(payload
            .get("macAddressCommand")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains(expected_windows_mac.as_str())));
        assert_eq!(
            payload.get("powershellMachineId").and_then(Value::as_str),
            Some(expected_machine_id.as_str())
        );
        assert_eq!(
            payload
                .get("powershellPlatformUuid")
                .and_then(Value::as_str),
            Some(expected_uuid.as_str())
        );
        assert_eq!(
            payload.get("powershellMacAddress").and_then(Value::as_str),
            Some(expected_windows_mac.as_str())
        );
    }

    #[cfg(target_os = "macos")]
    assert!(payload
        .get("ioregCommand")
        .and_then(Value::as_str)
        .is_some_and(|value| value.contains(expected_uuid.as_str())));
}

#[test]
fn run_claude_syncs_persistent_claude_identity_files() {
    let temp = tempfile::tempdir().unwrap();
    let fake = fixture_fake_claude_path();
    let state_root = temp.path().join("state");
    let home = temp.path().join("home");
    let statsig_dir = home.join(".claude/statsig");
    std::fs::create_dir_all(&statsig_dir).unwrap();
    std::fs::write(statsig_dir.join("statsig.stable_id.1"), "\"old-stable-id\"").unwrap();
    std::fs::write(home.join(".claude.json"), r#"{ "userID": "old-user-id" }"#).unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    let mut command = Command::cargo_bin("ccp").unwrap();
    command
        .env("CCP_STATE_ROOT", &state_root)
        .env("HOME", &home)
        .args(["run", "--profile", "work", "--"]);
    #[cfg(unix)]
    command.arg(&fake);
    #[cfg(windows)]
    command.args(["node"]).arg(&fake);
    command.assert().success();

    let identity_root = state_root.join("identities/work");
    let expected_stable_id = fs::read_to_string(identity_root.join("stable_id"))
        .unwrap()
        .trim()
        .to_string();
    let expected_user_id = fs::read_to_string(identity_root.join("user_id"))
        .unwrap()
        .trim()
        .to_string();

    let stable_id_file = fs::read_to_string(statsig_dir.join("statsig.stable_id.1")).unwrap();
    assert_eq!(stable_id_file, format!("\"{}\"", expected_stable_id));

    let claude_json: Value =
        serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
    assert_eq!(
        claude_json.get("userID").and_then(Value::as_str),
        Some(expected_user_id.as_str())
    );
}

#[test]
fn run_claude_uses_managed_claude_config_dir() {
    let temp = tempfile::tempdir().unwrap();
    let fake = fixture_fake_claude_path();
    let state_root = temp.path().join("state");
    let home = temp.path().join("home");
    let claude_dir = home.join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(
        claude_dir.join("settings.json"),
        r#"{
  "model": "sonnet",
  "env": {
    "ANTHROPIC_BASE_URL": "https://example.invalid",
    "ANTHROPIC_AUTH_TOKEN": "test-auth-token"
  }
}"#,
    )
    .unwrap();
    std::fs::write(home.join(".claude.json"), r#"{ "userID": "old-user-id" }"#).unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .env("HOME", &home)
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    let mut command = Command::cargo_bin("ccp").unwrap();
    command
        .env("CCP_STATE_ROOT", &state_root)
        .env("HOME", &home)
        .args(["run", "--profile", "work", "--"]);
    #[cfg(unix)]
    command.arg(&fake);
    #[cfg(windows)]
    command.args(["node"]).arg(&fake);

    let output = command.output().unwrap();
    assert!(output.status.success(), "{output:?}");

    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    let config_dir = payload
        .get("CLAUDE_CONFIG_DIR")
        .and_then(Value::as_str)
        .expect("wrapped claude should receive CLAUDE_CONFIG_DIR");
    assert!(
        config_dir.contains("claude-config"),
        "expected managed claude config dir, got {config_dir}"
    );

    let managed_root = PathBuf::from(config_dir);
    let settings = fs::read_to_string(managed_root.join("settings.json")).unwrap();
    assert!(settings.contains("\"model\": \"sonnet\""));
    assert!(settings.contains("example.invalid"));

    let expected_user_id = fs::read_to_string(state_root.join("identities/work/user_id"))
        .unwrap()
        .trim()
        .to_string();
    let claude_json = fs::read_to_string(managed_root.join(".claude.json")).unwrap();
    assert!(claude_json.contains(expected_user_id.as_str()));
}

#[test]
fn managed_claude_settings_are_profile_scoped_after_first_materialization() {
    let temp = tempfile::tempdir().unwrap();
    let fake = fixture_fake_claude_path();
    let state_root = temp.path().join("state");
    let home = temp.path().join("home");
    let claude_dir = home.join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(
        claude_dir.join("settings.json"),
        r#"{
  "model": "sonnet",
  "env": {
    "ANTHROPIC_BASE_URL": "https://first.example.invalid",
    "ANTHROPIC_AUTH_TOKEN": "first-token"
  }
}"#,
    )
    .unwrap();
    std::fs::write(home.join(".claude.json"), r#"{ "userID": "old-user-id" }"#).unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .env("HOME", &home)
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    let mut first_run = Command::cargo_bin("ccp").unwrap();
    first_run
        .env("CCP_STATE_ROOT", &state_root)
        .env("HOME", &home)
        .args(["run", "--profile", "work", "--"]);
    #[cfg(unix)]
    first_run.arg(&fake);
    #[cfg(windows)]
    first_run.args(["node"]).arg(&fake);

    let first_output = first_run.output().unwrap();
    assert!(first_output.status.success(), "{first_output:?}");
    let first_payload: Value = serde_json::from_slice(&first_output.stdout).unwrap();
    let managed_root = PathBuf::from(
        first_payload
            .get("CLAUDE_CONFIG_DIR")
            .and_then(Value::as_str)
            .unwrap(),
    );

    std::fs::write(
        claude_dir.join("settings.json"),
        r#"{
  "model": "opus",
  "env": {
    "ANTHROPIC_BASE_URL": "https://second.example.invalid",
    "ANTHROPIC_AUTH_TOKEN": "second-token"
  }
}"#,
    )
    .unwrap();

    let mut second_run = Command::cargo_bin("ccp").unwrap();
    second_run
        .env("CCP_STATE_ROOT", &state_root)
        .env("HOME", &home)
        .args(["run", "--profile", "work", "--"]);
    #[cfg(unix)]
    second_run.arg(&fake);
    #[cfg(windows)]
    second_run.args(["node"]).arg(&fake);

    let second_output = second_run.output().unwrap();
    assert!(second_output.status.success(), "{second_output:?}");

    let managed_settings = fs::read_to_string(managed_root.join("settings.json")).unwrap();
    assert!(managed_settings.contains("first.example.invalid"));
    assert!(managed_settings.contains("\"model\": \"sonnet\""));
    assert!(!managed_settings.contains("second.example.invalid"));
    assert!(!managed_settings.contains("\"model\": \"opus\""));
}

#[test]
fn managed_claude_settings_follow_explicit_profile_provider() {
    let temp = tempfile::tempdir().unwrap();
    let fake = fixture_fake_claude_path();
    let state_root = temp.path().join("state");
    let home = temp.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(home.join(".claude.json"), r#"{ "userID": "old-user-id" }"#).unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .env("HOME", &home)
        .args([
            "profile",
            "create",
            "work",
            "--adapter",
            "claude",
            "--base-url",
            "https://explicit.example.invalid",
            "--auth-token",
            "explicit-token",
        ])
        .assert()
        .success();

    let mut command = Command::cargo_bin("ccp").unwrap();
    command
        .env("CCP_STATE_ROOT", &state_root)
        .env("HOME", &home)
        .args(["run", "--profile", "work", "--"]);
    #[cfg(unix)]
    command.arg(&fake);
    #[cfg(windows)]
    command.args(["node"]).arg(&fake);

    let output = command.output().unwrap();
    assert!(output.status.success(), "{output:?}");

    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    let managed_root = PathBuf::from(
        payload
            .get("CLAUDE_CONFIG_DIR")
            .and_then(Value::as_str)
            .unwrap(),
    );
    let settings = fs::read_to_string(managed_root.join("settings.json")).unwrap();
    assert!(settings.contains("explicit.example.invalid"));
    assert!(settings.contains("explicit-token"));
}

#[test]
fn run_without_profile_uses_active_profile() {
    let temp = tempfile::tempdir().unwrap();
    let fake = fixture_fake_claude_path();
    let state_root = temp.path().join("state");
    let (proxy_url, _proxy_thread) = reachable_proxy_url();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .args([
            "profile",
            "create",
            "work",
            "--adapter",
            "claude",
            "--proxy",
            &proxy_url,
        ])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .args(["profile", "activate", "work"])
        .assert()
        .success();

    let mut command = Command::cargo_bin("ccp").unwrap();
    command
        .env("CCP_STATE_ROOT", &state_root)
        .args(["run", "--"]);
    #[cfg(unix)]
    command.arg(&fake);
    #[cfg(windows)]
    command.args(["node"]).arg(&fake);

    let output = command.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        payload.get("HTTPS_PROXY").and_then(Value::as_str),
        Some(proxy_url.as_str())
    );
}

#[test]
fn pause_and_resume_toggle_wrapping_for_active_profile() {
    let temp = tempfile::tempdir().unwrap();
    let fake = fixture_fake_claude_path();
    let state_root = temp.path().join("state");
    let (proxy_url, _proxy_thread) = reachable_proxy_url();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .args([
            "profile",
            "create",
            "work",
            "--adapter",
            "claude",
            "--proxy",
            &proxy_url,
        ])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .args(["profile", "activate", "work"])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .arg("pause")
        .assert()
        .success();

    let mut paused = Command::cargo_bin("ccp").unwrap();
    paused
        .env("CCP_STATE_ROOT", &state_root)
        .args(["run", "--"]);
    #[cfg(unix)]
    paused.arg(&fake);
    #[cfg(windows)]
    paused.args(["node"]).arg(&fake);
    let paused_output = paused.output().unwrap();
    assert!(paused_output.status.success(), "{paused_output:?}");
    let paused_payload: Value = serde_json::from_slice(&paused_output.stdout).unwrap();
    assert_eq!(
        paused_payload.get("HTTPS_PROXY").and_then(Value::as_str),
        None
    );
    assert_eq!(paused_payload.get("HOSTNAME").and_then(Value::as_str), None);

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .arg("resume")
        .assert()
        .success();

    let mut resumed = Command::cargo_bin("ccp").unwrap();
    resumed
        .env("CCP_STATE_ROOT", &state_root)
        .args(["run", "--"]);
    #[cfg(unix)]
    resumed.arg(&fake);
    #[cfg(windows)]
    resumed.args(["node"]).arg(&fake);
    let resumed_output = resumed.output().unwrap();
    assert!(resumed_output.status.success(), "{resumed_output:?}");
    let resumed_payload: Value = serde_json::from_slice(&resumed_output.stdout).unwrap();
    assert_eq!(
        resumed_payload.get("HTTPS_PROXY").and_then(Value::as_str),
        Some(proxy_url.as_str())
    );
}

#[test]
fn setup_generated_claude_wrapper_routes_through_ccp() {
    let temp = tempfile::tempdir().unwrap();
    let fake = fixture_fake_claude_path();
    let state_root = temp.path().join("state");
    let fake_bin = temp.path().join("fake-bin");
    let install_bin = temp.path().join("install-bin");
    let shell_rc = temp.path().join(".zshrc");
    let (proxy_url, _proxy_thread) = reachable_proxy_url();

    let real_claude = fake_real_claude_wrapper(&fake_bin, &fake);

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .env("PATH", path_with(std::slice::from_ref(&fake_bin)))
        .args([
            "setup",
            "--bin-dir",
            install_bin.to_str().unwrap(),
            "--shell-rc",
            shell_rc.to_str().unwrap(),
        ])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .args([
            "profile",
            "create",
            "work",
            "--adapter",
            "claude",
            "--proxy",
            &proxy_url,
        ])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .args(["profile", "activate", "work"])
        .assert()
        .success();

    let output = Command::new(install_bin.join(wrapper_name("claude")))
        .env("CCP_STATE_ROOT", &state_root)
        .env("PATH", path_with(&[install_bin.clone(), fake_bin.clone()]))
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        payload.get("HTTPS_PROXY").and_then(Value::as_str),
        Some(proxy_url.as_str())
    );

    let recorded_real = fs::read_to_string(state_root.join("config/real_claude_path"))
        .unwrap()
        .trim()
        .to_string();
    assert_eq!(recorded_real, real_claude.display().to_string());
}

fn path_with(extra: &[PathBuf]) -> String {
    std::env::join_paths(extra.iter().cloned().chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap()
    .to_string_lossy()
    .into_owned()
}

fn fake_real_claude_wrapper(dir: &Path, fake_claude_js: &Path) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(wrapper_name("claude"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(
            &path,
            format!(
                "#!/usr/bin/env bash\nexec node \"{}\" \"$@\"\n",
                fake_claude_js.display()
            ),
        )
        .unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
    }
    #[cfg(windows)]
    {
        fs::write(
            &path,
            format!("@echo off\r\nnode \"{}\" %*\r\n", fake_claude_js.display()),
        )
        .unwrap();
    }
    path
}

fn wrapper_name(base: &str) -> String {
    #[cfg(windows)]
    {
        match base {
            "claude" => "claude.cmd".to_string(),
            _ => "ccp.cmd".to_string(),
        }
    }
    #[cfg(not(windows))]
    {
        base.to_string()
    }
}

#[cfg(windows)]
fn expected_mac_address(raw: &str) -> String {
    raw.trim().replace(':', "-")
}
