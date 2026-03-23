use assert_cmd::Command;
use tempfile::tempdir;

#[cfg(unix)]
const EXIT_COMMAND: &[&str] = &["/bin/sh", "-c", "exit 42"];

#[cfg(windows)]
const EXIT_COMMAND: &[&str] = &["cmd", "/C", "exit", "42"];

#[cfg(unix)]
const SIGNAL_COMMAND: &[&str] = &["/bin/sh", "-c", "kill -TERM $$"];

#[test]
fn run_executes_generic_command_under_profile() {
    let temp = tempdir().unwrap();
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
        .success();
}

#[test]
fn run_propagates_exit_code() {
    let temp = tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "failing", "--adapter", "claude"])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["run", "--profile", "failing", "--"])
        .args(EXIT_COMMAND)
        .assert()
        .failure()
        .code(42);
}

#[cfg(unix)]
#[test]
fn run_propagates_signal_exit_code() {
    let temp = tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["profile", "create", "signaled", "--adapter", "claude"])
        .assert()
        .success();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args(["run", "--profile", "signaled", "--"])
        .args(SIGNAL_COMMAND)
        .assert()
        .failure()
        .code(128 + 15);
}
