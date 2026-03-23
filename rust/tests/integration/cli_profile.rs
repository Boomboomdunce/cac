use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn ccp_help_exits_successfully() {
    Command::cargo_bin("ccp").unwrap().arg("--help").assert().success();
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
