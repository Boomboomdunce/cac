use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;

#[test]
fn ccp_help_exits_successfully() {
    Command::cargo_bin("ccp")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
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
                .and(predicate::str::contains("profile existence")),
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

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["doctor", "--profile", "work"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("unsupported adapter"));
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
