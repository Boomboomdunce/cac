use assert_cmd::Command;
use core::{CapabilitySet, PrivacyPolicy, Profile, TargetAdapter};
use launcher::builder::{AdapterLaunchPolicy, LaunchPlanBuilder};
use sidecar::SIDECAR_PROTOCOL_VERSION;
use std::{fs, net::TcpListener, thread};
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
fn launch_plan_debug_redacts_proxy_credentials() {
    let profile = Profile::new(
        "claude-profile",
        "claude",
        PrivacyPolicy::default().with_proxy_url("https://alice:super-secret@proxy.example:8443"),
    );
    let adapter = TargetAdapter::new(
        profile.adapter.clone(),
        CapabilitySet::new(),
        CapabilitySet::new(),
        PrivacyPolicy::default(),
    );

    let plan = core::LaunchPlan::new(profile, adapter).expect("failed to build launch plan");
    let debug = format!("{plan:?}");

    assert!(debug.contains("https://alice:***@proxy.example:8443"));
    assert!(!debug.contains("super-secret"));
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

    assert!(err.to_string().contains("required capability"));
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
fn run_refuses_to_launch_when_proxy_is_unreachable() {
    let temp = tempdir().unwrap();
    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", temp.path())
        .args([
            "profile",
            "create",
            "isolated",
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
        .args(["run", "--profile", "isolated", "--", "env"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("proxy"));
}

#[test]
fn cli_defaults_state_root_to_home_ccp_rust_when_env_is_unset() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&cwd).unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .current_dir(&cwd)
        .env("HOME", &home)
        .env_remove("CCP_STATE_ROOT")
        .args(["profile", "create", "work", "--adapter", "claude"])
        .assert()
        .success();

    assert!(home.join(".ccp-rust/profiles/work.json").is_file());
    assert!(!cwd.join("ccp-state/profiles/work.json").exists());
}

#[test]
fn run_warns_when_sidecar_port_file_is_stale_before_falling_back() {
    let temp = tempdir().unwrap();
    let state_root = temp.path();
    let upstream = tcp_listener_fixture();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", state_root)
        .args([
            "profile",
            "create",
            "work",
            "--adapter",
            "claude",
            "--proxy",
            &format!("http://127.0.0.1:{}", upstream.port),
        ])
        .assert()
        .success();

    fs::create_dir_all(state_root.join("config")).unwrap();
    fs::write(state_root.join("config/sidecar_port"), "9\n").unwrap();

    Command::cargo_bin("ccp")
        .unwrap()
        .env("CCP_STATE_ROOT", state_root)
        .args(["run", "--profile", "work", "--", "env"])
        .assert()
        .success()
        .stderr(predicates::str::contains("warning: sidecar_port"));
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
            AdapterLaunchPolicy::new().with_runtime_hook_path("/tmp/ccp hooks/claude-preload.js"),
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
        .adapter_policy(AdapterLaunchPolicy::new().with_runtime_hook_path("/tmp/claude-preload.js"))
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

fn latest_env_value(execution: &launcher::LaunchPlanExecution, key: &str) -> Option<String> {
    execution
        .env_plan
        .iter()
        .filter(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.clone())
        .last()
}

struct TcpListenerFixture {
    port: u16,
}

fn tcp_listener_fixture() -> TcpListenerFixture {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let _join = thread::spawn(move || {
        for _ in 0..4 {
            if listener.accept().is_err() {
                break;
            }
        }
    });
    TcpListenerFixture { port }
}
