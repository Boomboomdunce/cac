use assert_cmd::Command;
use core::{CapabilitySet, PrivacyPolicy, Profile, TargetAdapter};
use launcher::builder::{AdapterLaunchPolicy, LaunchPlanBuilder};
use sidecar::SIDECAR_PROTOCOL_VERSION;
use tempfile::tempdir;

#[cfg(unix)]
const EXIT_COMMAND: &[&str] = &["/bin/sh", "-c", "exit 42"];

#[cfg(windows)]
const EXIT_COMMAND: &[&str] = &["cmd", "/C", "exit", "42"];

#[cfg(unix)]
const SIGNAL_COMMAND: &[&str] = &["/bin/sh", "-c", "kill -TERM $$"];

#[cfg(unix)]
const UNSET_COMMAND: &[&str] = &["/bin/sh", "-c", "test -z \"$ANTHROPIC_API_KEY\""];

#[cfg(windows)]
const UNSET_COMMAND: &[&str] = &[
    "cmd",
    "/C",
    "if not defined ANTHROPIC_API_KEY (exit /b 0) else (exit /b 1)",
];

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
fn launcher_refuses_when_required_capabilities_are_missing() {
    let profile = Profile::new("claude-profile", "claude", PrivacyPolicy::default());
    let adapter = TargetAdapter::new(
        profile.adapter.clone(),
        CapabilitySet::from(["node_preload", "sidecar", "kernel_driver"]),
        CapabilitySet::new(),
        PrivacyPolicy::default(),
    );

    let err = LaunchPlanBuilder::new()
        .profile(profile)
        .adapter(adapter)
        .command(vec!["true".to_string()])
        .build()
        .unwrap_err();

    assert!(err
        .to_string()
        .contains("required capability"));
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

#[test]
fn builder_quotes_runtime_hook_paths_in_node_options() {
    let profile = Profile::new("claude-profile", "claude", PrivacyPolicy::default());
    let adapter = TargetAdapter::new(
        profile.adapter.clone(),
        CapabilitySet::new(),
        CapabilitySet::new(),
        PrivacyPolicy::default(),
    );

    let execution = LaunchPlanBuilder::new()
        .profile(profile)
        .adapter(adapter)
        .command(vec!["true".to_string()])
        .adapter_policy(
            AdapterLaunchPolicy::new()
                .with_runtime_hook_path("/tmp/ccp hooks/claude-preload.js"),
        )
        .build()
        .expect("failed to build launch plan");

    let node_options = latest_env_value(&execution, "NODE_OPTIONS").expect("missing NODE_OPTIONS");
    assert!(node_options.contains("--require=\"/tmp/ccp hooks/claude-preload.js\""));
}

#[test]
fn builder_merges_staged_node_options_with_runtime_hook_requirement() {
    let profile = Profile::new("claude-profile", "claude", PrivacyPolicy::default());
    let adapter = TargetAdapter::new(
        profile.adapter.clone(),
        CapabilitySet::new(),
        CapabilitySet::new(),
        PrivacyPolicy::default(),
    );

    let execution = LaunchPlanBuilder::new()
        .profile(profile)
        .adapter(adapter)
        .command(vec!["true".to_string()])
        .env_var("NODE_OPTIONS", "--trace-warnings")
        .adapter_policy(
            AdapterLaunchPolicy::new().with_runtime_hook_path("/tmp/claude-preload.js"),
        )
        .build()
        .expect("failed to build launch plan");

    let node_options = latest_env_value(&execution, "NODE_OPTIONS").expect("missing NODE_OPTIONS");
    assert!(node_options.contains("--trace-warnings"));
    assert!(node_options.contains("--require=/tmp/claude-preload.js"));
}

#[test]
fn adapter_unset_wins_over_staged_env_var() {
    let profile = Profile::new("claude-profile", "claude", PrivacyPolicy::default());
    let adapter = TargetAdapter::new(
        profile.adapter.clone(),
        CapabilitySet::new(),
        CapabilitySet::new(),
        PrivacyPolicy::default(),
    );

    let execution = LaunchPlanBuilder::new()
        .profile(profile)
        .adapter(adapter)
        .command(UNSET_COMMAND.iter().map(|item| item.to_string()).collect())
        .env_var("ANTHROPIC_API_KEY", "secret")
        .adapter_policy(AdapterLaunchPolicy::new().with_env_unset("ANTHROPIC_API_KEY"))
        .build()
        .expect("failed to build launch plan");

    let status = launcher::exec::execute(&execution).expect("failed to execute launch plan");
    assert!(status.success(), "staged env var should have been unset");
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

fn latest_env_value(
    execution: &launcher::LaunchPlanExecution,
    key: &str,
) -> Option<String> {
    execution
        .env_plan
        .iter()
        .filter(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.clone())
        .last()
}
