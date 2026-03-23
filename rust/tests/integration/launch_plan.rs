use assert_cmd::Command;
use core::{CapabilitySet, PrivacyPolicy, Profile, TargetAdapter};
use launcher::builder::LaunchPlanBuilder;
use sidecar::SIDECAR_PROTOCOL_VERSION;
use tempfile::tempdir;

#[cfg(unix)]
const EXIT_COMMAND: &[&str] = &["/bin/sh", "-c", "exit 42"];

#[cfg(windows)]
const EXIT_COMMAND: &[&str] = &["cmd", "/C", "exit", "42"];

#[cfg(unix)]
const SIGNAL_COMMAND: &[&str] = &["/bin/sh", "-c", "kill -TERM $$"];

#[test]
fn claude_launch_plan_requires_sidecar() {
    let profile = Profile::new("claude-profile", "claude", PrivacyPolicy::default());
    let adapter = TargetAdapter::new(
        profile.adapter.clone(),
        CapabilitySet::new(),
        CapabilitySet::new(),
        PrivacyPolicy::default(),
    );

    let plan = LaunchPlanBuilder::new()
        .profile(profile)
        .adapter(adapter)
        .command(vec!["true".to_string()])
        .build()
        .expect("failed to build launch plan");

    assert!(
        plan.session.sidecar_required,
        "claude launch plan should require a sidecar session"
    );
    assert_eq!(plan.session.adapter, "claude");
    assert_eq!(plan.session.protocol_version, SIDECAR_PROTOCOL_VERSION);
}

#[test]
fn claude_launch_plan_normalizes_injected_session_metadata() {
    let profile = Profile::new("claude-profile", "claude", PrivacyPolicy::default());
    let adapter = TargetAdapter::new(
        profile.adapter.clone(),
        CapabilitySet::new(),
        CapabilitySet::new(),
        PrivacyPolicy::default(),
    );

    let plan = LaunchPlanBuilder::new()
        .profile(profile)
        .adapter(adapter)
        .command(vec!["true".to_string()])
        .session(launcher::Session {
            id: "manual-session".into(),
            adapter: "generic".into(),
            sidecar_required: false,
            protocol_version: 0,
        })
        .build()
        .expect("failed to build launch plan");

    assert_eq!(plan.session.id, "manual-session");
    assert!(plan.session.sidecar_required);
    assert_eq!(plan.session.adapter, "claude");
    assert_eq!(plan.session.protocol_version, SIDECAR_PROTOCOL_VERSION);
}

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
