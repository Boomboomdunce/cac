use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

#[test]
fn ccp_help_exits_successfully() {
    Command::cargo_bin("ccp")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
}

#[test]
fn ccp_version_subcommand_exits_successfully() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .arg("version")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("ccp 0.1.0")
                .and(predicate::str::contains("安装方式: cargo run/test (Rust)")),
        );
}

#[test]
fn run_without_profiles_prints_setup_guidance() {
    let temp = tempfile::tempdir().unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["run", "--", "env"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("no active profile")
                .and(predicate::str::contains(
                    "ccp profile create work --adapter claude --proxy",
                ))
                .and(predicate::str::contains("ccp profile activate work"))
                .and(predicate::str::contains("ccp setup")),
        );
}

#[test]
fn ccp_mitm_prepare_creates_root_ca_and_bundle() {
    let temp = tempfile::tempdir().unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["mitm", "prepare"])
        .assert()
        .success()
        .stdout(predicate::str::contains("prepared MITM capture materials"));

    assert!(temp.path().join("certs/mitm/root_ca.pem").is_file());
    assert!(temp.path().join("certs/mitm/root_ca_key.pem").is_file());
    assert!(temp
        .path()
        .join("certs/mitm/node_extra_ca_bundle.pem")
        .is_file());
}

#[cfg(target_os = "macos")]
#[test]
fn ccp_mitm_trust_status_and_untrust_use_security_integration() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let keychain_dir = home.join("Library/Keychains");
    fs::create_dir_all(&keychain_dir).unwrap();
    let state_file = temp.path().join("fake-security-cert.pem");
    let script = write_fake_security_script(temp.path(), &state_file);

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .env("HOME", &home)
        .env("CCP_SECURITY_BIN", &script)
        .args(["mitm", "trust"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "trusted in the macOS login keychain",
        ));

    assert!(state_file.is_file());

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .env("HOME", &home)
        .env("CCP_SECURITY_BIN", &script)
        .args(["mitm", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "MITM system trust: MITM root is trusted",
        ));

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .env("HOME", &home)
        .env("CCP_SECURITY_BIN", &script)
        .args(["mitm", "untrust"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not trusted"));

    assert!(!state_file.exists());
}

#[test]
fn run_with_proxyless_profile_warns_about_missing_proxy_configuration() {
    let temp = tempfile::tempdir().unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["run", "--profile", "work", "--", "env"])
        .assert()
        .success()
        .stderr(predicate::str::contains("has no proxy configured"));
}

#[test]
fn profile_create_writes_profile_to_state_root() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    let profile_file = temp.path().join("profiles/work.json");
    assert!(profile_file.exists());
    let contents = fs::read_to_string(profile_file).unwrap();
    assert!(contents.contains("\"name\": \"work\""));

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "list"])
        .assert()
        .stdout(predicate::str::contains("work (claude)"));
}

#[test]
fn profile_create_persists_proxy_configuration() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args([
            "profile",
            "create",
            "work",
            "--adapter",
            "claude",
            "--proxy",
            "https://alice:super-secret@proxy.example:8443",
        ])
        .assert()
        .success();

    let profile_file = temp.path().join("profiles/work.json");
    let contents = fs::read_to_string(profile_file).unwrap();
    assert!(contents.contains("\"proxy_url\": \"https://alice:super-secret@proxy.example:8443\""));
    assert!(temp.path().join("certs/ca/ca_cert.pem").is_file());
    assert!(temp.path().join("certs/work/client_cert.pem").is_file());
    assert!(temp.path().join("certs/work/client_key.pem").is_file());
}

#[test]
fn profile_create_accepts_host_port_proxy_shorthand() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args([
            "profile",
            "create",
            "work",
            "--adapter",
            "claude",
            "--proxy",
            "127.0.0.1:8080",
        ])
        .assert()
        .success();

    let profile_file = temp.path().join("profiles/work.json");
    let contents = fs::read_to_string(profile_file).unwrap();
    assert!(contents.contains("\"proxy_url\": \"http://127.0.0.1:8080\""));
}

#[test]
fn profile_create_accepts_host_port_user_pass_proxy_shorthand() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args([
            "profile",
            "create",
            "work",
            "--adapter",
            "claude",
            "--proxy",
            "127.0.0.1:8080:alice:super-secret",
        ])
        .assert()
        .success();

    let profile_file = temp.path().join("profiles/work.json");
    let contents = fs::read_to_string(profile_file).unwrap();
    assert!(contents.contains("\"proxy_url\": \"http://alice:super-secret@127.0.0.1:8080\""));
}

#[test]
fn profile_create_snapshots_claude_provider_from_user_settings() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let claude_dir = home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(
        claude_dir.join("settings.json"),
        r#"{
  "env": {
    "ANTHROPIC_BASE_URL": "https://code.nextcloud.games",
    "ANTHROPIC_AUTH_TOKEN": "top-secret-token"
  }
}"#,
    )
    .unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .env("HOME", &home)
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    let profile_file = temp.path().join("profiles/work.json");
    let contents: Value = serde_json::from_str(&fs::read_to_string(profile_file).unwrap()).unwrap();
    assert_eq!(
        contents
            .get("claude")
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.get("base_url"))
            .and_then(Value::as_str),
        Some("https://code.nextcloud.games")
    );
    assert_eq!(
        contents
            .get("claude")
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.get("auth_token"))
            .and_then(Value::as_str),
        Some("top-secret-token")
    );
}

#[test]
fn profile_create_derives_tz_and_lang_from_proxy_exit_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let ipify = one_shot_http_server("198.51.100.77");
    let geo = one_shot_http_server(r#"{"timezone":"Asia/Tokyo","countryCode":"JP"}"#);

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .env(
            "CCP_IPIFY_URL",
            format!("http://127.0.0.1:{}/ipify", ipify.port),
        )
        .env(
            "CCP_GEOIP_URL_TEMPLATE",
            format!(
                "http://127.0.0.1:{}/json/{{ip}}?fields=timezone,countryCode",
                geo.port
            ),
        )
        .args([
            "profile",
            "create",
            "work",
            "--adapter",
            "claude",
            "--proxy",
            &format!("http://127.0.0.1:{}", ipify.port),
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(temp.path().join("identities/work/tz"))
            .unwrap()
            .trim(),
        "Asia/Tokyo"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("identities/work/lang"))
            .unwrap()
            .trim(),
        "ja_JP.UTF-8"
    );

    ipify.join.join().unwrap();
    geo.join.join().unwrap();
}

#[test]
fn profile_create_materializes_identity_files() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    for relative in [
        "identities/work/uuid",
        "identities/work/stable_id",
        "identities/work/user_id",
        "identities/work/machine_id",
        "identities/work/hostname",
        "identities/work/mac_address",
        "identities/work/tz",
        "identities/work/lang",
    ] {
        assert!(
            temp.path().join(relative).is_file(),
            "missing identity file {relative}"
        );
    }
}

#[test]
fn ccp_doctor_reports_missing_profile() {
    let temp = tempfile::tempdir().unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["doctor", "--profile", "work"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("profile existence"));
}

#[test]
fn ccp_doctor_reports_success_for_existing_profile() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["doctor", "--profile", "work"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("State root layout")
                .and(predicate::str::contains("profile existence"))
                .and(predicate::str::contains("identity materials"))
                .and(predicate::str::contains("mTLS materials"))
                .and(predicate::str::contains("MITM materials"))
                .and(predicate::str::contains("MITM system trust"))
                .and(predicate::str::contains("proxy exit IP"))
                .and(predicate::str::contains("runtime self-audit"))
                .and(predicate::str::contains("runtime live self-audit")),
        );
}

#[test]
fn ccp_doctor_outputs_json_when_requested() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    let output = Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["doctor", "--profile", "work", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report.get("ok").and_then(Value::as_bool).unwrap_or(false));
    let checks = report.get("checks").and_then(Value::as_array).unwrap();
    assert!(!checks.is_empty());
    assert!(checks
        .iter()
        .any(|check| { check.get("name").and_then(Value::as_str) == Some("proxy exit IP") }));
    assert!(checks
        .iter()
        .any(|check| { check.get("name").and_then(Value::as_str) == Some("dns blocking") }));
    assert!(checks
        .iter()
        .any(|check| { check.get("name").and_then(Value::as_str) == Some("runtime self-audit") }));
    assert!(checks.iter().any(|check| {
        check.get("name").and_then(Value::as_str) == Some("runtime live self-audit")
    }));
    let mtls = checks
        .iter()
        .find(|check| check.get("name").and_then(Value::as_str) == Some("mTLS materials"))
        .expect("missing mTLS materials check");
    let mtls_message = mtls
        .get("message")
        .and_then(Value::as_str)
        .expect("mTLS materials check should include details");
    assert!(mtls_message.contains("ccp-client-work"));
    assert!(mtls_message.contains("expires"));
}

#[test]
fn profile_show_redacts_proxy_credentials() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    let profile_file = temp.path().join("profiles/work.json");
    fs::write(
        &profile_file,
        r#"{
  "name": "work",
  "adapter": "claude",
  "claude": {
    "provider": {
      "base_url": "https://code.nextcloud.games",
      "auth_token": "top-secret-token",
      "api_key": "top-secret-api-key"
    }
  },
  "policy": {
    "proxy_url": "https://alice:super-secret@proxy.example:8443"
  }
}"#,
    )
    .unwrap();

    let output = Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "show", "work"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("https://alice:***@proxy.example:8443"));
    assert!(!stdout.contains("super-secret"));
    assert!(stdout.contains("\"base_url\": \"https://code.nextcloud.games\""));
    assert!(stdout.contains("\"auth_token\": \"***\""));
    assert!(stdout.contains("\"api_key\": \"***\""));
    assert!(!stdout.contains("top-secret-token"));
    assert!(!stdout.contains("top-secret-api-key"));
}

#[test]
fn profile_activate_syncs_claude_identity_files_immediately() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let statsig_dir = home.join(".claude/statsig");
    fs::create_dir_all(&statsig_dir).unwrap();
    fs::write(statsig_dir.join("statsig.stable_id.1"), "\"old-stable-id\"").unwrap();
    fs::write(home.join(".claude.json"), r#"{ "userID": "old-user-id" }"#).unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .env("HOME", &home)
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .env("HOME", &home)
        .args(["profile", "activate", "work"])
        .assert()
        .success();

    let expected_stable_id = fs::read_to_string(temp.path().join("identities/work/stable_id"))
        .unwrap()
        .trim()
        .to_string();
    let expected_user_id = fs::read_to_string(temp.path().join("identities/work/user_id"))
        .unwrap()
        .trim()
        .to_string();

    assert_eq!(
        fs::read_to_string(statsig_dir.join("statsig.stable_id.1")).unwrap(),
        format!("\"{expected_stable_id}\"")
    );
    let claude_json: Value =
        serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
    assert_eq!(
        claude_json.get("userID").and_then(Value::as_str),
        Some(expected_user_id.as_str())
    );
}

#[test]
fn ccp_doctor_rejects_unsupported_adapter() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "experimental"])
        .assert()
        .success();

    let output = Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["doctor", "--profile", "work"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("unsupported adapter"));
    assert!(
        !stdout.contains("platform capability support: OK"),
        "doctor should not report platform capability support as OK for an unsupported adapter"
    );
}

#[test]
fn ccp_doctor_warns_on_missing_secret_directory() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    let identities = temp.path().join("identities");
    std::fs::remove_dir_all(&identities).unwrap();

    let output = Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["doctor", "--profile", "work"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("secret permission sanity: WARNING"));
}

#[test]
fn ccp_doctor_fails_when_identity_materials_are_missing() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    fs::remove_file(temp.path().join("identities/work/hostname")).unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["doctor", "--profile", "work"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("identity materials"));
}

#[test]
fn ccp_doctor_fails_when_mtls_materials_are_missing() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    fs::remove_file(temp.path().join("certs/work/client_cert.pem")).unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["doctor", "--profile", "work"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("mTLS materials"));
}

#[test]
fn ccp_doctor_fails_when_mtls_certificate_chain_is_invalid() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    fs::write(
        temp.path().join("certs/work/client_cert.pem"),
        "not a certificate\n",
    )
    .unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["doctor", "--profile", "work"])
        .assert()
        .failure()
        .stdout(
            predicate::str::contains("mTLS materials")
                .and(predicate::str::contains("certificate verification failed")),
        );
}

#[test]
fn ccp_doctor_fails_when_proxy_is_unreachable() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args([
            "profile",
            "create",
            "work",
            "--adapter",
            "claude",
            "--proxy",
            "http://127.0.0.1:9",
        ])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["doctor", "--profile", "work"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("proxy reachability"));
}

#[test]
fn ccp_doctor_warns_when_runtime_assets_are_missing() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    fs::write(temp.path().join("hooks/claude-preload.js"), "placeholder").unwrap();
    fs::remove_file(temp.path().join("hooks/claude-preload.js")).unwrap();

    let output = Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["doctor", "--profile", "work"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("runtime self-audit: WARNING"));
}

#[test]
fn profile_create_generates_bash_compatible_certificate_parameters() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    let ca_key = temp.path().join("certs/ca/ca_key.pem");
    let ca_cert = temp.path().join("certs/ca/ca_cert.pem");
    let client_key = temp.path().join("certs/work/client_key.pem");
    let client_cert = temp.path().join("certs/work/client_cert.pem");

    let ca_key_text = openssl_stdout(&["pkey", "-in", ca_key.to_str().unwrap(), "-text", "-noout"]);
    assert!(ca_key_text.contains("Private-Key: (4096 bit"));

    let client_key_text = openssl_stdout(&[
        "pkey",
        "-in",
        client_key.to_str().unwrap(),
        "-text",
        "-noout",
    ]);
    assert!(client_key_text.contains("Private-Key: (2048 bit"));

    let ca_subject = openssl_stdout(&[
        "x509",
        "-in",
        ca_cert.to_str().unwrap(),
        "-noout",
        "-subject",
    ]);
    assert!(ca_subject.contains("CN = ccp-privacy-ca") || ca_subject.contains("CN=ccp-privacy-ca"));

    let client_subject = openssl_stdout(&[
        "x509",
        "-in",
        client_cert.to_str().unwrap(),
        "-noout",
        "-subject",
    ]);
    assert!(
        client_subject.contains("CN = ccp-client-work")
            || client_subject.contains("CN=ccp-client-work")
    );

    let current_year = current_utc_year();
    let ca_end_year = certificate_end_year(&ca_cert);
    let client_end_year = certificate_end_year(&client_cert);
    assert!(
        (current_year + 9..=current_year + 11).contains(&ca_end_year),
        "expected CA to expire roughly 10 years out, got {ca_end_year}"
    );
    assert!(
        (current_year..=current_year + 2).contains(&client_end_year),
        "expected client cert to expire roughly 1 year out, got {client_end_year}"
    );
}

#[test]
fn profile_delete_removes_profile_and_materials() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "activate", "work"])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "delete", "work"])
        .assert()
        .success();

    assert!(!temp.path().join("profiles/work.json").exists());
    assert!(!temp.path().join("identities/work").exists());
    assert!(!temp.path().join("certs/work").exists());
    assert!(!temp.path().join("config/current_profile").exists());
}

#[test]
fn profile_activate_recovers_from_empty_claude_json() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let statsig_dir = home.join(".claude/statsig");
    fs::create_dir_all(&statsig_dir).unwrap();
    fs::write(statsig_dir.join("statsig.stable_id.1"), "\"old-stable-id\"").unwrap();
    fs::write(home.join(".claude.json"), "").unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .env("HOME", &home)
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .env("HOME", &home)
        .args(["profile", "activate", "work"])
        .assert()
        .success();

    let expected_user_id = fs::read_to_string(temp.path().join("identities/work/user_id"))
        .unwrap()
        .trim()
        .to_string();
    let claude_json: Value =
        serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
    assert_eq!(
        claude_json.get("userID").and_then(Value::as_str),
        Some(expected_user_id.as_str())
    );
}

#[test]
fn setup_writes_install_shims_and_shell_block() {
    let temp = tempfile::tempdir().unwrap();
    let fake_bin = temp.path().join("fake-bin");
    let install_bin = temp.path().join("install-bin");
    let shell_rc = temp.path().join(".zshrc");
    let real_claude = fake_claude_executable(&fake_bin);

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path().join("state"))
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

    assert!(install_bin.join(wrapper_name("claude")).is_file());
    assert!(install_bin.join(wrapper_name("ccp")).is_file());
    assert!(shell_rc.is_file());

    let shell_contents = fs::read_to_string(&shell_rc).unwrap();
    assert!(shell_contents.contains("# >>> ccp >>>"));
    assert!(shell_contents.contains(install_bin.to_str().unwrap()));

    let real_claude_record =
        fs::read_to_string(temp.path().join("state/config/real_claude_path")).unwrap();
    assert_eq!(real_claude_record.trim(), real_claude.to_str().unwrap());
    assert!(temp.path().join("state/config/install.json").is_file());
}

#[test]
fn uninstall_removes_install_artifacts_and_state_root() {
    let temp = tempfile::tempdir().unwrap();
    let fake_bin = temp.path().join("fake-bin");
    let install_bin = temp.path().join("install-bin");
    let shell_rc = temp.path().join(".zshrc");
    let state_root = temp.path().join("state");
    fake_claude_executable(&fake_bin);

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
        .args(["uninstall"])
        .assert()
        .success();

    assert!(!install_bin.join(wrapper_name("claude")).exists());
    assert!(!install_bin.join(wrapper_name("ccp")).exists());
    let shell_contents = fs::read_to_string(&shell_rc).unwrap_or_default();
    assert!(!shell_contents.contains("# >>> ccp >>>"));
    assert!(!state_root.exists());
}

#[cfg(target_os = "macos")]
#[test]
fn uninstall_also_removes_installed_mitm_system_trust() {
    let temp = tempfile::tempdir().unwrap();
    let fake_bin = temp.path().join("fake-bin");
    let install_bin = temp.path().join("install-bin");
    let shell_rc = temp.path().join(".zshrc");
    let state_root = temp.path().join("state");
    let home = temp.path().join("home");
    let keychain_dir = home.join("Library/Keychains");
    let state_file = temp.path().join("fake-security-cert.pem");
    fs::create_dir_all(&keychain_dir).unwrap();
    let script = write_fake_security_script(temp.path(), &state_file);
    fake_claude_executable(&fake_bin);

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .env("PATH", path_with(std::slice::from_ref(&fake_bin)))
        .env("HOME", &home)
        .env("CCP_SECURITY_BIN", &script)
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
        .env("HOME", &home)
        .env("CCP_SECURITY_BIN", &script)
        .args(["mitm", "trust"])
        .assert()
        .success();

    assert!(state_file.is_file());

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", &state_root)
        .env("HOME", &home)
        .env("CCP_SECURITY_BIN", &script)
        .args(["uninstall"])
        .assert()
        .success();

    assert!(!state_file.exists());
    assert!(!state_root.exists());
}

fn path_with(extra: &[PathBuf]) -> String {
    std::env::join_paths(extra.iter().cloned().chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap()
    .to_string_lossy()
    .into_owned()
}

fn fake_claude_executable(dir: &Path) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(wrapper_name("claude"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&path, "#!/usr/bin/env bash\nexit 0\n").unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
    }
    #[cfg(windows)]
    {
        fs::write(&path, "@echo off\r\nexit /b 0\r\n").unwrap();
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

struct HttpFixture {
    port: u16,
    join: thread::JoinHandle<()>,
}

fn one_shot_http_server(body: &'static str) -> HttpFixture {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let join = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0_u8; 2048];
        let _ = stream.read(&mut buffer);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });
    HttpFixture { port, join }
}

#[cfg(target_os = "macos")]
fn write_fake_security_script(dir: &Path, state_file: &Path) -> PathBuf {
    let script = dir.join("fake-security.sh");
    fs::write(
        &script,
        format!(
            "#!/bin/zsh\nset -eu\ncmd=\"$1\"\nshift\nstate=\"{}\"\ncase \"$cmd\" in\n  find-certificate)\n    if [[ -f \"$state\" ]]; then\n      cat \"$state\"\n    fi\n    ;;\n  add-trusted-cert)\n    cert=\"${{@: -1}}\"\n    cp \"$cert\" \"$state\"\n    ;;\n  delete-certificate)\n    rm -f \"$state\"\n    ;;\n  *)\n    echo \"unsupported\" >&2\n    exit 1\n    ;;\nesac\n",
            state_file.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();
    script
}

fn openssl_stdout(args: &[&str]) -> String {
    let output = std::process::Command::new("openssl")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "openssl {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn current_utc_year() -> i32 {
    let output = std::process::Command::new("date")
        .args(["-u", "+%Y"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap()
}

fn certificate_end_year(path: &Path) -> i32 {
    let enddate = openssl_stdout(&["x509", "-in", path.to_str().unwrap(), "-noout", "-enddate"]);
    enddate
        .split_whitespace()
        .rev()
        .find(|part| part.chars().all(|ch| ch.is_ascii_digit()))
        .expect("openssl enddate output should include year")
        .parse()
        .unwrap()
}
