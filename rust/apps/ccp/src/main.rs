use anyhow::{Context, Result};
use ccp::{default_state_root, home_dir, inspect_setup_status, install};
use clap::{Parser, Subcommand};
use claude_adapter::{claude_adapter, ADAPTER_NAME};
use core::{
    proxy_host_port, CapabilitySet, ClaudeProviderConfig, PrivacyPolicy, Profile, TargetAdapter,
};
use doctor::{CheckResult, DoctorConfig, DoctorReport};
use install::SetupConfig;
use launcher::{
    builder::{AdapterLaunchPolicy, LaunchPlanBuilder},
    exec,
};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;
use std::{env, fs, path::PathBuf, process};
use store::{
    certificate_material, ensure_mitm_certificates, ensure_profile_certificates,
    ensure_profile_identity, ensure_profile_identity_seeded, ensure_runtime_shims,
    install_mitm_system_trust, load_profile_identity, materialize_blocked_hosts_file,
    materialize_managed_claude_config, mitm_certificate_material, mitm_system_trust_status,
    remove_mitm_system_trust, snapshot_user_claude_provider, MitmCertificateMaterial, ProfileStore,
    StateLayout,
};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

#[derive(Parser)]
#[command(
    name = "ccp",
    about = "Command Privacy Proxy",
    version,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Version,
    Profile(ProfileGroup),
    Run(RunCommand),
    Doctor(DoctorCommand),
    Mitm(MitmGroup),
    Setup(SetupCommand),
    Uninstall,
    Pause,
    Resume,
}

#[derive(Parser)]
struct ProfileGroup {
    #[command(subcommand)]
    command: Option<ProfileCommand>,
}

#[derive(Subcommand)]
enum ProfileCommand {
    Create {
        name: String,
        #[arg(long)]
        adapter: String,
        #[arg(long)]
        proxy: Option<String>,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long)]
        auth_token: Option<String>,
        #[arg(long)]
        api_key: Option<String>,
    },
    Activate {
        name: String,
    },
    Delete {
        name: String,
    },
    Show {
        name: String,
    },
    List,
}

#[derive(Parser)]
#[command(trailing_var_arg = true)]
struct RunCommand {
    #[arg(long)]
    profile: Option<String>,
    #[arg(required = true, num_args = 1..)]
    command: Vec<String>,
}

#[derive(Parser)]
struct DoctorCommand {
    #[arg(long)]
    profile: String,

    #[arg(long)]
    json: bool,
}

#[derive(Parser)]
struct MitmGroup {
    #[command(subcommand)]
    command: Option<MitmCommand>,
}

#[derive(Subcommand)]
enum MitmCommand {
    Prepare,
    Status,
    Trust,
    Untrust,
}

#[derive(Parser)]
struct SetupCommand {
    #[arg(long)]
    bin_dir: Option<PathBuf>,

    #[arg(long)]
    shell_rc: Option<PathBuf>,

    #[arg(long, hide = true)]
    ccp_bin: Option<PathBuf>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        for cause in err.chain().skip(1) {
            eprintln!("caused by: {cause}");
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Version) => {
            println!("ccp {}", env!("CARGO_PKG_VERSION"));
            println!("安装方式: {}", version_install_method_label());
            Ok(())
        }
        Some(Commands::Profile(group)) => {
            if let Some(profile_command) = group.command {
                let layout =
                    StateLayout::new(state_root()?).context("initializing CCP state layout")?;
                let store = ProfileStore::new(layout);
                profile_command_handler(&store, profile_command)
            } else {
                println!("profile command expected");
                Ok(())
            }
        }
        Some(Commands::Run(run_cmd)) => {
            let status = run_command_handler(run_cmd).context("running command")?;
            if status.success() {
                Ok(())
            } else {
                exit_with_status(status);
            }
        }
        Some(Commands::Doctor(cmd)) => {
            let root = state_root().context("determining CCP state root for doctor")?;
            let mut report = doctor::run(DoctorConfig::new(root.clone(), cmd.profile.clone()));
            augment_doctor_with_live_audit(&mut report, &root, &cmd.profile);

            if cmd.json {
                let json = serde_json::to_string_pretty(&report)?;
                println!("{json}");
            } else {
                println!("{}", report.render_human());
            }

            if report.is_ok() {
                Ok(())
            } else {
                Err(anyhow::anyhow!("doctor reported failing checks"))
            }
        }
        Some(Commands::Mitm(group)) => {
            if let Some(command) = group.command {
                mitm_command_handler(command)
            } else {
                println!("mitm command expected");
                Ok(())
            }
        }
        Some(Commands::Setup(cmd)) => setup_command_handler(cmd),
        Some(Commands::Uninstall) => uninstall_command_handler(),
        Some(Commands::Pause) => pause_command_handler(),
        Some(Commands::Resume) => resume_command_handler(),
        None => Ok(()),
    }
}

fn profile_command_handler(store: &ProfileStore, command: ProfileCommand) -> Result<()> {
    let runtime_state = store::RuntimeStateStore::new(store.layout().clone());
    match command {
        ProfileCommand::Create {
            name,
            adapter,
            proxy,
            base_url,
            auth_token,
            api_key,
        } => {
            let mut policy = PrivacyPolicy::default();
            if let Some(proxy) = proxy {
                policy = policy.with_proxy_url(normalize_proxy_input(proxy.as_str()));
            }
            let mut profile = Profile::new(name.clone(), adapter, policy);
            if profile.adapter.eq_ignore_ascii_case(ADAPTER_NAME) {
                if let Some(provider) = resolve_claude_provider(base_url, auth_token, api_key)
                    .context("resolving Claude provider configuration")?
                {
                    profile = profile.with_claude_provider(provider);
                }
            }
            let saved = store.save_profile(&profile).context("saving profile")?;
            let locale = infer_profile_locale(profile.policy.proxy_url())
                .context("inferring profile locale")?;
            ensure_profile_identity_seeded(
                store.layout(),
                &name,
                Some(locale.timezone.as_str()),
                Some(locale.lang.as_str()),
            )
            .context("materializing identity materials")?;
            ensure_profile_certificates(store.layout(), &name)
                .context("materializing certificate materials")?;
            materialize_adapter_assets(&profile, store.layout())
                .context("materializing adapter runtime assets")?;
            println!("{}", saved.display());
            Ok(())
        }
        ProfileCommand::Activate { name } => {
            let profile = store.load_profile(&name).context("loading profile")?;
            runtime_state
                .set_active_profile(&name)
                .context("setting active profile")?;
            runtime_state
                .set_paused(false)
                .context("clearing paused state")?;
            sync_claude_identity_files_for_profile(store.layout(), &profile)
                .context("syncing Claude identity files for active profile")?;
            println!("active profile: {name}");
            Ok(())
        }
        ProfileCommand::Delete { name } => {
            let canonical_name =
                store::canonical_name(&name).context("validating profile name for deletion")?;
            store
                .delete_profile(&canonical_name)
                .context("deleting profile metadata")?;
            remove_dir_if_exists(store.layout().identities_dir().join(&canonical_name))
                .context("deleting identity materials")?;
            remove_dir_if_exists(store.layout().certs_dir().join(&canonical_name))
                .context("deleting mTLS materials")?;

            if runtime_state
                .active_profile()
                .context("loading active profile state")?
                .as_deref()
                == Some(canonical_name.as_str())
            {
                runtime_state
                    .clear_active_profile()
                    .context("clearing active profile state")?;
                runtime_state
                    .set_paused(false)
                    .context("clearing paused state")?;
            }

            println!("deleted profile: {canonical_name}");
            Ok(())
        }
        ProfileCommand::Show { name } => {
            let profile = store.load_profile(&name).context("loading profile")?;
            let json = serde_json::to_string_pretty(&profile.redacted())?;
            println!("{json}");
            Ok(())
        }
        ProfileCommand::List => {
            let profiles = store.list_profiles().context("listing profiles")?;
            let active_profile = runtime_state
                .active_profile()
                .context("loading active profile state")?;
            let paused = runtime_state.is_paused();
            for profile in profiles {
                let active_marker = if active_profile.as_deref() == Some(profile.name.as_str()) {
                    if paused {
                        " [active, paused]"
                    } else {
                        " [active]"
                    }
                } else {
                    ""
                };
                println!("{} ({}){}", profile.name, profile.adapter, active_marker);
            }
            Ok(())
        }
    }
}

fn run_command_handler(RunCommand { profile, command }: RunCommand) -> Result<process::ExitStatus> {
    let layout = StateLayout::new(state_root()?).context("initializing CCP state layout")?;
    let store = ProfileStore::new(layout.clone());
    let runtime_state = store::RuntimeStateStore::new(layout.clone());
    let setup_status = inspect_setup_status(layout.root());

    if runtime_state.is_paused() {
        return execute_unwrapped(&command).context("executing command while paused");
    }

    let profile_name = match profile {
        Some(profile_name) => profile_name,
        None => runtime_state
            .active_profile()
            .context("loading active profile state")?
            .ok_or_else(|| anyhow::anyhow!(render_no_active_profile_guidance(&setup_status)))?,
    };

    let profile = store
        .load_profile(&profile_name)
        .context("loading profile")?;
    if profile.policy.proxy_url().is_none() {
        eprintln!(
            "warning: profile '{}' has no proxy configured; traffic capture and proxy-based protections will be inactive. Recreate the profile with `ccp profile create <new-name> --adapter claude --proxy http://host:port` or use CCP Desktop to edit it.",
            profile_name
        );
    }
    let (adapter, adapter_policy) =
        resolve_adapter_policy(&profile, &layout).context("resolving adapter policy")?;

    let plan = LaunchPlanBuilder::new()
        .profile(profile)
        .adapter(adapter)
        .adapter_policy(adapter_policy)
        .command(command)
        .build()
        .context("building launch plan")?;

    let status = exec::execute(&plan).context("executing launch plan")?;
    Ok(status)
}

fn pause_command_handler() -> Result<()> {
    let layout = StateLayout::new(state_root()?).context("initializing CCP state layout")?;
    let runtime_state = store::RuntimeStateStore::new(layout);
    runtime_state
        .set_paused(true)
        .context("marking runtime as paused")?;
    println!("privacy wrapper paused");
    Ok(())
}

fn mitm_command_handler(command: MitmCommand) -> Result<()> {
    let layout = StateLayout::new(state_root()?).context("initializing CCP state layout")?;

    match command {
        MitmCommand::Prepare => {
            let material = ensure_mitm_certificates(&layout).context("preparing MITM materials")?;
            println!("prepared MITM capture materials");
            println!("root CA: {}", material.ca_cert.display());
            println!("node bundle: {}", material.node_ca_bundle.display());
            Ok(())
        }
        MitmCommand::Status => {
            let material = mitm_certificate_material(&layout);
            let cert_ready = material.ca_cert.is_file()
                && material.ca_key.is_file()
                && material.node_ca_bundle.is_file();
            let trust = mitm_system_trust_status(&layout).context("checking MITM system trust")?;
            println!(
                "MITM certificates: {}",
                if cert_ready { "ready" } else { "missing" }
            );
            println!("MITM system trust: {}", trust.message);
            Ok(())
        }
        MitmCommand::Trust => {
            let trust =
                install_mitm_system_trust(&layout).context("installing MITM system trust")?;
            println!("{}", trust.message);
            Ok(())
        }
        MitmCommand::Untrust => {
            let trust = remove_mitm_system_trust(&layout).context("removing MITM system trust")?;
            println!("{}", trust.message);
            Ok(())
        }
    }
}

fn setup_command_handler(cmd: SetupCommand) -> Result<()> {
    let layout = StateLayout::new(state_root()?).context("initializing CCP state layout")?;
    let home_dir =
        home_dir().ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
    let bin_dir = cmd.bin_dir.unwrap_or_else(|| home_dir.join("bin"));
    let shell_rc = cmd.shell_rc.or_else(install::detect_shell_rc);
    let ccp_bin_path = match cmd.ccp_bin {
        Some(path) => path,
        None => env::current_exe().context("resolving ccp binary path")?,
    };

    let metadata = install::setup(
        &layout,
        SetupConfig {
            bin_dir,
            shell_rc,
            ccp_bin_path,
        },
    )
    .context("performing setup")?;

    println!("installed wrappers:");
    for path in metadata.generated_paths {
        println!("{}", path.display());
    }
    Ok(())
}

fn uninstall_command_handler() -> Result<()> {
    let root = state_root().context("determining CCP state root for uninstall")?;
    if root.exists() {
        if let Ok(layout) = StateLayout::new(root.clone()) {
            if let Err(err) = remove_mitm_system_trust(&layout) {
                eprintln!("warning: failed to remove MITM system trust: {err}");
            }
        }
    }
    install::uninstall(&root).context("performing uninstall")?;
    println!("uninstalled ccp artifacts");
    Ok(())
}

fn resume_command_handler() -> Result<()> {
    let layout = StateLayout::new(state_root()?).context("initializing CCP state layout")?;
    let runtime_state = store::RuntimeStateStore::new(layout);
    let active_profile = runtime_state
        .active_profile()
        .context("loading active profile state")?;
    if active_profile.is_none() {
        return Err(anyhow::anyhow!(
            "cannot resume without an active profile; use `ccp profile activate <name>`"
        ));
    }
    runtime_state
        .set_paused(false)
        .context("clearing paused state")?;
    println!("privacy wrapper resumed");
    Ok(())
}

#[cfg(unix)]
fn exit_with_status(status: process::ExitStatus) -> ! {
    if let Some(code) = status.code() {
        process::exit(code);
    }
    if let Some(signal) = status.signal() {
        process::exit(128 + signal);
    }
    process::exit(1);
}

#[cfg(not(unix))]
fn exit_with_status(status: process::ExitStatus) -> ! {
    if let Some(code) = status.code() {
        process::exit(code);
    }
    process::exit(1);
}

fn state_root() -> Result<PathBuf, std::io::Error> {
    default_state_root()
}

fn version_install_method_label() -> &'static str {
    if state_root()
        .ok()
        .is_some_and(|root| root.join("config/install.json").is_file())
    {
        "wrapper (Rust)"
    } else if env::current_exe().ok().is_some_and(|path| {
        path.components()
            .any(|component| component.as_os_str() == "target")
    }) {
        "cargo run/test (Rust)"
    } else {
        "standalone binary (Rust)"
    }
}

fn augment_doctor_with_live_audit(
    report: &mut DoctorReport,
    root: &std::path::Path,
    profile_name: &str,
) {
    const CHECK_NAME: &str = "runtime live self-audit";

    let check = match live_audit_check(root, profile_name) {
        Ok(check) => check,
        Err(err) => CheckResult::warning(
            CHECK_NAME,
            Some(format!("failed to prepare live audit: {err}")),
        ),
    };
    report.add_check(check);
}

fn live_audit_check(root: &std::path::Path, profile_name: &str) -> Result<CheckResult> {
    const CHECK_NAME: &str = "runtime live self-audit";

    let layout = StateLayout::new(root.to_path_buf()).context("initializing state layout")?;
    let store = ProfileStore::new(layout.clone());
    let profile = store
        .load_profile(profile_name)
        .context("loading profile for live audit")?;

    if !profile.adapter.eq_ignore_ascii_case(ADAPTER_NAME) {
        return Ok(CheckResult::warning(
            CHECK_NAME,
            Some(format!(
                "no live runtime audit implemented for adapter '{}'",
                profile.adapter
            )),
        ));
    }

    let (adapter, adapter_policy) = match resolve_live_audit_adapter_policy(&profile, &layout) {
        Ok(parts) => parts,
        Err(err) => {
            return Ok(CheckResult::warning(
                CHECK_NAME,
                Some(format!("unable to assemble runtime environment: {err}")),
            ));
        }
    };

    let execution = match LaunchPlanBuilder::new()
        .profile(profile)
        .adapter(adapter)
        .adapter_policy(adapter_policy)
        .command(vec![
            "node".to_string(),
            "-e".to_string(),
            live_audit_node_script().to_string(),
        ])
        .build()
    {
        Ok(execution) => execution,
        Err(err) => {
            return Ok(CheckResult::warning(
                CHECK_NAME,
                Some(format!("unable to build wrapped node launch plan: {err}")),
            ));
        }
    };

    let output = match execute_with_output(&execution) {
        Ok(output) => output,
        Err(err) => {
            return Ok(CheckResult::warning(
                CHECK_NAME,
                Some(format!("unable to execute wrapped node audit: {err}")),
            ));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("wrapped node exited with status {}", output.status)
        };
        return Ok(CheckResult::error(CHECK_NAME, Some(detail)));
    }

    let payload: LiveAuditPayload =
        serde_json::from_slice(&output.stdout).context("parsing live audit payload")?;
    let problems = validate_live_audit_payload(&payload);
    if problems.is_empty() {
        Ok(CheckResult::ok(
            CHECK_NAME,
            Some(
                "wrapped node launch confirmed env hardening, Node proxying, and DNS blocking"
                    .to_string(),
            ),
        ))
    } else {
        Ok(CheckResult::error(CHECK_NAME, Some(problems.join("; "))))
    }
}

fn resolve_live_audit_adapter_policy(
    profile: &Profile,
    layout: &StateLayout,
) -> Result<(TargetAdapter, AdapterLaunchPolicy)> {
    let identity = load_profile_identity(layout, &profile.name).context("loading identity")?;
    let cert_material = certificate_material(layout, &profile.name);
    let mitm_material = store::mitm_certificate_material(layout);
    let missing_certificates = [
        ("CA cert", cert_material.ca_cert.as_path()),
        ("CA key", cert_material.ca_key.as_path()),
        ("client cert", cert_material.client_cert.as_path()),
        ("client key", cert_material.client_key.as_path()),
        ("MITM CA cert", mitm_material.ca_cert.as_path()),
        ("MITM CA key", mitm_material.ca_key.as_path()),
        ("Node CA bundle", mitm_material.node_ca_bundle.as_path()),
    ]
    .into_iter()
    .filter_map(|(label, path)| (!path.is_file()).then_some(label))
    .collect::<Vec<_>>();
    if !missing_certificates.is_empty() {
        anyhow::bail!(
            "missing certificate files: {}",
            missing_certificates.join(", ")
        );
    }

    let shims = ensure_runtime_shims(layout).context("materializing runtime shims")?;
    let runtime_hook = layout.hooks_dir().join("claude-preload.js");
    let blocked_hosts_path = layout.config_dir().join("blocked_hosts");
    let missing_runtime_assets = [runtime_hook.as_path(), blocked_hosts_path.as_path()]
        .into_iter()
        .filter(|path| !path.is_file())
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if !missing_runtime_assets.is_empty() {
        anyhow::bail!(
            "missing runtime assets: {}",
            missing_runtime_assets.join(", ")
        );
    }

    let adapter = claude_adapter();
    let mut policy = AdapterLaunchPolicy::new().with_runtime_hook_path(runtime_hook);
    for (key, value) in adapter.environment_overrides() {
        policy = policy.with_env_override(key.clone(), value.clone());
    }
    for key in adapter.environment_unsets() {
        policy = policy.with_env_unset(key.clone());
    }
    policy = with_identity_environment(policy, &identity, &shims.dir)
        .context("building identity environment")?;
    policy = policy.with_env_override("HOSTALIASES", blocked_hosts_path.display().to_string());
    policy = with_mtls_environment(policy, profile, &cert_material, &mitm_material);

    Ok((adapter.target_adapter().clone(), policy))
}

fn execute_with_output(
    execution: &launcher::builder::LaunchPlanExecution,
) -> Result<process::Output, launcher::builder::LaunchError> {
    let mut command_iter = execution.command.iter();
    let program = match command_iter.next() {
        Some(cmd) => cmd,
        None => return Err(launcher::builder::LaunchError::MissingCommand),
    };

    let mut cmd = process::Command::new(program);
    cmd.args(command_iter);
    for key in execution.env_plan.removals() {
        cmd.env_remove(key);
    }
    for (key, value) in execution.env_plan.iter() {
        cmd.env(key, value);
    }

    cmd.output()
        .map_err(launcher::builder::LaunchError::Execution)
}

fn live_audit_node_script() -> &'static str {
    r#"
const dns = require('dns');
const has = (key) => Object.prototype.hasOwnProperty.call(process.env, key);
const payload = {
  CLAUDE_CODE_ENABLE_TELEMETRY: has('CLAUDE_CODE_ENABLE_TELEMETRY') ? process.env.CLAUDE_CODE_ENABLE_TELEMETRY : null,
  NODE_USE_ENV_PROXY: has('NODE_USE_ENV_PROXY') ? process.env.NODE_USE_ENV_PROXY : null,
  DO_NOT_TRACK: has('DO_NOT_TRACK') ? process.env.DO_NOT_TRACK : null,
  OTEL_SDK_DISABLED: has('OTEL_SDK_DISABLED') ? process.env.OTEL_SDK_DISABLED : null,
  OTEL_TRACES_EXPORTER: has('OTEL_TRACES_EXPORTER') ? process.env.OTEL_TRACES_EXPORTER : null,
  OTEL_METRICS_EXPORTER: has('OTEL_METRICS_EXPORTER') ? process.env.OTEL_METRICS_EXPORTER : null,
  OTEL_LOGS_EXPORTER: has('OTEL_LOGS_EXPORTER') ? process.env.OTEL_LOGS_EXPORTER : null,
  SENTRY_DSN: has('SENTRY_DSN') ? process.env.SENTRY_DSN : null,
  DISABLE_ERROR_REPORTING: has('DISABLE_ERROR_REPORTING') ? process.env.DISABLE_ERROR_REPORTING : null,
  DISABLE_BUG_COMMAND: has('DISABLE_BUG_COMMAND') ? process.env.DISABLE_BUG_COMMAND : null,
  CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: has('CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC') ? process.env.CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC : null,
  TELEMETRY_DISABLED: has('TELEMETRY_DISABLED') ? process.env.TELEMETRY_DISABLED : null,
  DISABLE_TELEMETRY: has('DISABLE_TELEMETRY') ? process.env.DISABLE_TELEMETRY : null,
  CCP_RUNTIME_HOOK: has('CCP_RUNTIME_HOOK') ? process.env.CCP_RUNTIME_HOOK : null,
  HOSTALIASES: has('HOSTALIASES') ? process.env.HOSTALIASES : null,
  ANTHROPIC_BASE_URL: has('ANTHROPIC_BASE_URL') ? process.env.ANTHROPIC_BASE_URL : null,
  ANTHROPIC_AUTH_TOKEN: has('ANTHROPIC_AUTH_TOKEN') ? process.env.ANTHROPIC_AUTH_TOKEN : null,
  ANTHROPIC_API_KEY: has('ANTHROPIC_API_KEY') ? process.env.ANTHROPIC_API_KEY : null,
};

dns.lookup('statsig.anthropic.com', (err) => {
  payload.dnsErrorCode = err && err.code ? err.code : null;
  payload.dnsBlocked = payload.dnsErrorCode === 'ECONNREFUSED';
  console.log(JSON.stringify(payload));
});
"#
}

fn validate_live_audit_payload(payload: &LiveAuditPayload) -> Vec<String> {
    let mut problems = Vec::new();

    if payload.claude_code_enable_telemetry.as_deref() != Some("") {
        problems.push("CLAUDE_CODE_ENABLE_TELEMETRY is not cleared".to_string());
    }
    if payload.node_use_env_proxy.as_deref() != Some("1") {
        problems.push("NODE_USE_ENV_PROXY is not 1".to_string());
    }
    if payload.do_not_track.as_deref() != Some("1") {
        problems.push("DO_NOT_TRACK is not 1".to_string());
    }
    if payload.otel_sdk_disabled.as_deref() != Some("true") {
        problems.push("OTEL_SDK_DISABLED is not true".to_string());
    }
    if payload.otel_traces_exporter.as_deref() != Some("none") {
        problems.push("OTEL_TRACES_EXPORTER is not none".to_string());
    }
    if payload.otel_metrics_exporter.as_deref() != Some("none") {
        problems.push("OTEL_METRICS_EXPORTER is not none".to_string());
    }
    if payload.otel_logs_exporter.as_deref() != Some("none") {
        problems.push("OTEL_LOGS_EXPORTER is not none".to_string());
    }
    if payload.sentry_dsn.as_deref() != Some("") {
        problems.push("SENTRY_DSN is not cleared".to_string());
    }
    if payload.disable_error_reporting.as_deref() != Some("1") {
        problems.push("DISABLE_ERROR_REPORTING is not 1".to_string());
    }
    if payload.disable_bug_command.as_deref() != Some("1") {
        problems.push("DISABLE_BUG_COMMAND is not 1".to_string());
    }
    if payload.claude_code_disable_nonessential_traffic.as_deref() != Some("1") {
        problems.push("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC is not 1".to_string());
    }
    if payload.telemetry_disabled.as_deref() != Some("1") {
        problems.push("TELEMETRY_DISABLED is not 1".to_string());
    }
    if payload.disable_telemetry.as_deref() != Some("1") {
        problems.push("DISABLE_TELEMETRY is not 1".to_string());
    }
    if payload
        .ccp_runtime_hook
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        problems.push("CCP_RUNTIME_HOOK is missing".to_string());
    }
    if payload
        .hostaliases
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        problems.push("HOSTALIASES is missing".to_string());
    }
    if payload.anthropic_base_url.is_some() {
        problems.push("ANTHROPIC_BASE_URL should be unset".to_string());
    }
    if payload.anthropic_auth_token.is_some() {
        problems.push("ANTHROPIC_AUTH_TOKEN should be unset".to_string());
    }
    if payload.anthropic_api_key.is_some() {
        problems.push("ANTHROPIC_API_KEY should be unset".to_string());
    }
    if !payload.dns_blocked {
        let code = payload.dns_error_code.as_deref().unwrap_or("none");
        problems.push(format!(
            "dns.lookup(statsig.anthropic.com) was not blocked with ECONNREFUSED (got {code})"
        ));
    }

    problems
}

fn resolve_adapter_policy(
    profile: &Profile,
    layout: &StateLayout,
) -> Result<(TargetAdapter, AdapterLaunchPolicy)> {
    ensure_profile_identity(layout, &profile.name).context("materializing identity materials")?;
    let identity = load_profile_identity(layout, &profile.name).context("loading identity")?;
    let cert_material = ensure_profile_certificates(layout, &profile.name)
        .context("materializing certificate materials")?;
    let mitm_material =
        ensure_mitm_certificates(layout).context("materializing MITM certificate materials")?;
    let shims = ensure_runtime_shims(layout).context("materializing runtime shims")?;

    if profile.adapter.eq_ignore_ascii_case(ADAPTER_NAME) {
        let adapter = claude_adapter();
        let managed_config = materialize_managed_claude_config(layout, profile)
            .context("materializing managed Claude config")?;
        sync_claude_identity_files(&identity, Some(&managed_config.root))
            .context("syncing Claude identity files")?;
        let (runtime_hook, blocked_hosts_path) = materialize_claude_runtime_assets(layout)
            .context("materializing Claude runtime assets")?;

        let mut policy = AdapterLaunchPolicy::new()
            .with_runtime_hook_path(runtime_hook)
            .with_env_override(
                "CLAUDE_CONFIG_DIR",
                managed_config.root.display().to_string(),
            );
        for (key, value) in adapter.environment_overrides() {
            policy = policy.with_env_override(key.clone(), value.clone());
        }
        for key in adapter.environment_unsets() {
            policy = policy.with_env_unset(key.clone());
        }
        policy = with_identity_environment(policy, &identity, &shims.dir)
            .context("building identity environment")?;
        policy = policy.with_env_override("HOSTALIASES", blocked_hosts_path.display().to_string());
        policy = with_mtls_environment(policy, profile, &cert_material, &mitm_material);

        Ok((adapter.target_adapter().clone(), policy))
    } else {
        Ok((
            TargetAdapter::new(
                profile.adapter.clone(),
                CapabilitySet::new(),
                CapabilitySet::new(),
                PrivacyPolicy::default(),
            ),
            with_mtls_environment(
                with_identity_environment(AdapterLaunchPolicy::new(), &identity, &shims.dir)
                    .context("building identity environment")?,
                profile,
                &cert_material,
                &mitm_material,
            ),
        ))
    }
}

fn sync_claude_identity_files_for_profile(layout: &StateLayout, profile: &Profile) -> Result<()> {
    if !profile.adapter.eq_ignore_ascii_case(ADAPTER_NAME) {
        return Ok(());
    }

    let identity = load_profile_identity(layout, &profile.name).context("loading identity")?;
    sync_claude_identity_files(&identity, None)
}

fn materialize_adapter_assets(profile: &Profile, layout: &StateLayout) -> Result<()> {
    if profile.adapter.eq_ignore_ascii_case(ADAPTER_NAME) {
        let _ = materialize_claude_runtime_assets(layout)?;
    }
    Ok(())
}

fn with_mtls_environment(
    mut policy: AdapterLaunchPolicy,
    profile: &Profile,
    cert_material: &store::CertificateMaterial,
    mitm_material: &MitmCertificateMaterial,
) -> AdapterLaunchPolicy {
    policy = policy
        .with_env_override(
            "CCP_MTLS_CERT",
            cert_material.client_cert.display().to_string(),
        )
        .with_env_override(
            "CCP_MTLS_KEY",
            cert_material.client_key.display().to_string(),
        )
        .with_env_override("CCP_MTLS_CA", cert_material.ca_cert.display().to_string())
        .with_env_override(
            "CAC_MTLS_CERT",
            cert_material.client_cert.display().to_string(),
        )
        .with_env_override(
            "CAC_MTLS_KEY",
            cert_material.client_key.display().to_string(),
        )
        .with_env_override("CAC_MTLS_CA", cert_material.ca_cert.display().to_string())
        .with_env_override(
            "NODE_EXTRA_CA_CERTS",
            mitm_material.node_ca_bundle.display().to_string(),
        );

    if let Some(proxy_url) = profile.policy.proxy_url() {
        if let Some(proxy_host_port) = proxy_host_port(proxy_url) {
            policy = policy
                .with_env_override("CCP_PROXY_HOST", proxy_host_port.clone())
                .with_env_override("CAC_PROXY_HOST", proxy_host_port);
        }
    }

    policy
}

fn with_identity_environment(
    mut policy: AdapterLaunchPolicy,
    identity: &store::ProfileIdentity,
    shim_dir: &std::path::Path,
) -> Result<AdapterLaunchPolicy> {
    let path_value = prepend_to_path(shim_dir)?;
    policy = policy
        .with_env_override("HOSTNAME", identity.hostname.clone())
        .with_env_override("COMPUTERNAME", identity.hostname.clone())
        .with_env_override("TZ", identity.tz.clone())
        .with_env_override("LANG", identity.lang.clone())
        .with_env_override("CCP_FAKE_HOSTNAME", identity.hostname.clone())
        .with_env_override("CCP_FAKE_MACHINE_ID", identity.machine_id.clone())
        .with_env_override("CCP_FAKE_PLATFORM_UUID", identity.uuid.clone())
        .with_env_override("CCP_FAKE_MAC_ADDRESS", identity.mac_address.clone())
        .with_env_override("PATH", path_value);

    Ok(policy)
}

fn prepend_to_path(path: &std::path::Path) -> Result<String> {
    let mut paths = vec![path.to_path_buf()];
    if let Some(existing) = env::var_os("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    let joined = env::join_paths(paths).context("joining runtime shim path")?;
    Ok(joined.to_string_lossy().into_owned())
}

fn sync_claude_identity_files(
    identity: &store::ProfileIdentity,
    managed_config_root: Option<&std::path::Path>,
) -> Result<()> {
    let Some(home_dir) = home_dir() else {
        return Ok(());
    };

    sync_statsig_dir(
        &home_dir.join(".claude").join("statsig"),
        &identity.stable_id,
    )
    .context("syncing user Claude statsig stable id")?;
    sync_claude_json_file(&home_dir.join(".claude.json"), &identity.user_id)
        .context("syncing user Claude JSON identity")?;

    if let Some(root) = managed_config_root {
        fs::create_dir_all(root.join("statsig"))
            .with_context(|| format!("creating {}", root.join("statsig").display()))?;
        sync_statsig_dir(&root.join("statsig"), &identity.stable_id)
            .context("syncing managed Claude statsig stable id")?;
        sync_claude_json_file(&root.join(".claude.json"), &identity.user_id)
            .context("syncing managed Claude JSON identity")?;
    }

    Ok(())
}

fn sync_statsig_dir(statsig_dir: &std::path::Path, stable_id: &str) -> Result<()> {
    if let Ok(entries) = fs::read_dir(statsig_dir) {
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if name.starts_with("statsig.stable_id.") {
                fs::write(&path, format!("\"{}\"", stable_id))
                    .with_context(|| format!("writing {}", path.display()))?;
            }
        }
    }
    Ok(())
}

fn sync_claude_json_file(claude_json: &std::path::Path, user_id: &str) -> Result<()> {
    let mut document = if claude_json.is_file() {
        let contents = fs::read_to_string(claude_json)
            .with_context(|| format!("reading {}", claude_json.display()))?;
        if contents.trim().is_empty() {
            Value::Object(serde_json::Map::new())
        } else {
            serde_json::from_str::<Value>(&contents)
                .with_context(|| format!("parsing {}", claude_json.display()))?
        }
    } else {
        Value::Object(serde_json::Map::new())
    };

    match &mut document {
        Value::Object(map) => {
            map.insert("userID".to_string(), Value::String(user_id.to_string()));
        }
        _ => {
            return Err(anyhow::anyhow!(
                "{} must contain a JSON object",
                claude_json.display()
            ));
        }
    }

    let rendered = serde_json::to_string_pretty(&document)?;
    fs::write(claude_json, format!("{rendered}\n"))
        .with_context(|| format!("writing {}", claude_json.display()))?;
    Ok(())
}

fn render_no_active_profile_guidance(setup_status: &ccp::SetupStatus) -> String {
    if setup_status.profiles.is_empty() {
        let mut guidance = String::from(
            "no active profile; no profiles are configured yet. Create one with `ccp profile create work --adapter claude --proxy http://host:port`, then activate it with `ccp profile activate work`.",
        );
        if !setup_status.wrappers_installed {
            guidance.push_str(" Install global wrappers with `ccp setup` once you are ready to route `claude` automatically.");
        }
        return guidance;
    }

    let available = setup_status.profiles.join(", ");
    format!(
        "no active profile; available profiles: {available}. Activate one with `ccp profile activate <name>` or pass `--profile <name>` to this run."
    )
}

fn execute_unwrapped(command: &[String]) -> Result<process::ExitStatus> {
    let mut command_iter = command.iter();
    let program = command_iter
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing command to execute"))?;
    let mut child = process::Command::new(program);
    child.args(command_iter);
    child.status().context("executing unwrapped command")
}

fn remove_dir_if_exists(path: PathBuf) -> Result<()> {
    match fs::remove_dir_all(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("removing {}", path.display())),
    }
}

fn materialize_runtime_hook(path: PathBuf, contents: &str) -> Result<PathBuf> {
    fs::write(&path, contents)
        .with_context(|| format!("writing runtime hook to {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&path, permissions)?;
    }
    Ok(path)
}

fn materialize_claude_runtime_assets(layout: &StateLayout) -> Result<(PathBuf, PathBuf)> {
    let adapter = claude_adapter();
    let runtime_hook = materialize_runtime_hook(
        layout.hooks_dir().join("claude-preload.js"),
        adapter.runtime_hook_bundle().contents(),
    )
    .context("materializing Claude runtime hook")?;
    let blocked_hosts_path = materialize_blocked_hosts_file(layout, adapter.blocked_hosts())
        .context("materializing blocked hosts file")?;
    Ok((runtime_hook, blocked_hosts_path))
}

fn resolve_claude_provider(
    base_url: Option<String>,
    auth_token: Option<String>,
    api_key: Option<String>,
) -> Result<Option<ClaudeProviderConfig>> {
    if base_url.is_some() || auth_token.is_some() || api_key.is_some() {
        return Ok(Some(ClaudeProviderConfig {
            base_url,
            auth_token,
            api_key,
        }));
    }

    snapshot_user_claude_provider().context("reading user Claude provider settings")
}

fn normalize_proxy_input(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.contains("://") {
        return trimmed.to_string();
    }

    let parts = trimmed.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        [host, port] if !host.is_empty() && !port.is_empty() => {
            format!("http://{host}:{port}")
        }
        [host, port, user, pass]
            if !host.is_empty() && !port.is_empty() && !user.is_empty() && !pass.is_empty() =>
        {
            format!("http://{user}:{pass}@{host}:{port}")
        }
        _ => trimmed.to_string(),
    }
}

#[derive(Clone, Debug)]
struct ProfileLocale {
    timezone: String,
    lang: String,
}

#[derive(Debug, Deserialize)]
struct GeoResponse {
    #[serde(default)]
    timezone: String,
    #[serde(rename = "countryCode", default)]
    country_code: String,
}

#[derive(Debug, Deserialize)]
struct LiveAuditPayload {
    #[serde(rename = "CLAUDE_CODE_ENABLE_TELEMETRY")]
    claude_code_enable_telemetry: Option<String>,
    #[serde(rename = "NODE_USE_ENV_PROXY")]
    node_use_env_proxy: Option<String>,
    #[serde(rename = "DO_NOT_TRACK")]
    do_not_track: Option<String>,
    #[serde(rename = "OTEL_SDK_DISABLED")]
    otel_sdk_disabled: Option<String>,
    #[serde(rename = "OTEL_TRACES_EXPORTER")]
    otel_traces_exporter: Option<String>,
    #[serde(rename = "OTEL_METRICS_EXPORTER")]
    otel_metrics_exporter: Option<String>,
    #[serde(rename = "OTEL_LOGS_EXPORTER")]
    otel_logs_exporter: Option<String>,
    #[serde(rename = "SENTRY_DSN")]
    sentry_dsn: Option<String>,
    #[serde(rename = "DISABLE_ERROR_REPORTING")]
    disable_error_reporting: Option<String>,
    #[serde(rename = "DISABLE_BUG_COMMAND")]
    disable_bug_command: Option<String>,
    #[serde(rename = "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")]
    claude_code_disable_nonessential_traffic: Option<String>,
    #[serde(rename = "TELEMETRY_DISABLED")]
    telemetry_disabled: Option<String>,
    #[serde(rename = "DISABLE_TELEMETRY")]
    disable_telemetry: Option<String>,
    #[serde(rename = "CCP_RUNTIME_HOOK")]
    ccp_runtime_hook: Option<String>,
    #[serde(rename = "HOSTALIASES")]
    hostaliases: Option<String>,
    #[serde(rename = "ANTHROPIC_BASE_URL")]
    anthropic_base_url: Option<String>,
    #[serde(rename = "ANTHROPIC_AUTH_TOKEN")]
    anthropic_auth_token: Option<String>,
    #[serde(rename = "ANTHROPIC_API_KEY")]
    anthropic_api_key: Option<String>,
    #[serde(rename = "dnsErrorCode")]
    dns_error_code: Option<String>,
    #[serde(rename = "dnsBlocked", default)]
    dns_blocked: bool,
}

fn infer_profile_locale(proxy_url: Option<&str>) -> Result<ProfileLocale> {
    let default = ProfileLocale {
        timezone: "America/New_York".to_string(),
        lang: "en_US.UTF-8".to_string(),
    };

    let Some(proxy_url) = proxy_url else {
        return Ok(default);
    };

    let exit_ip = detect_exit_ip(proxy_url).ok();
    let Some(exit_ip) = exit_ip.filter(|value| !value.trim().is_empty()) else {
        return Ok(default);
    };

    let geo = fetch_geo_metadata(exit_ip.trim()).ok();
    let Some(geo) = geo else {
        return Ok(default);
    };

    let timezone = if geo.timezone.trim().is_empty() {
        default.timezone
    } else {
        geo.timezone
    };
    let lang = language_for_country(geo.country_code.as_str()).to_string();

    Ok(ProfileLocale { timezone, lang })
}

fn detect_exit_ip(proxy_url: &str) -> Result<String> {
    let ipify_url =
        env::var("CCP_IPIFY_URL").unwrap_or_else(|_| "https://api.ipify.org".to_string());
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .proxy(reqwest::Proxy::all(proxy_url)?)
        .build()
        .context("building proxy-aware HTTP client")?;
    let response = client
        .get(ipify_url)
        .send()
        .context("fetching exit IP")?
        .error_for_status()
        .context("exit IP service returned non-success status")?;
    response.text().context("reading exit IP response")
}

fn fetch_geo_metadata(ip: &str) -> Result<GeoResponse> {
    let template = env::var("CCP_GEOIP_URL_TEMPLATE")
        .unwrap_or_else(|_| "http://ip-api.com/json/{ip}?fields=timezone,countryCode".to_string());
    let url = template.replace("{ip}", ip);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .context("building geo lookup client")?;
    let response = client
        .get(url)
        .send()
        .context("fetching geo metadata")?
        .error_for_status()
        .context("geo lookup returned non-success status")?;
    response.json().context("parsing geo metadata response")
}

fn language_for_country(country_code: &str) -> &'static str {
    match country_code.trim().to_ascii_uppercase().as_str() {
        "JP" => "ja_JP.UTF-8",
        "CN" => "zh_CN.UTF-8",
        "TW" => "zh_TW.UTF-8",
        "KR" => "ko_KR.UTF-8",
        "GB" => "en_GB.UTF-8",
        "AU" => "en_AU.UTF-8",
        "CA" => "en_CA.UTF-8",
        _ => "en_US.UTF-8",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use launcher::builder::LaunchPlanBuilder;
    use tempfile::tempdir;

    #[test]
    fn with_identity_environment_sets_windows_friendly_hostname_variables() {
        let shim_dir = tempdir().expect("tempdir should exist");
        let identity = store::ProfileIdentity {
            uuid: "FAKE-UUID".to_string(),
            stable_id: "stable".to_string(),
            user_id: "user".to_string(),
            machine_id: "machine-guid".to_string(),
            hostname: "host-fake".to_string(),
            mac_address: "02:aa:bb:cc:dd:ee".to_string(),
            tz: "America/New_York".to_string(),
            lang: "en_US.UTF-8".to_string(),
        };

        let policy =
            with_identity_environment(AdapterLaunchPolicy::new(), &identity, shim_dir.path())
                .expect("identity env should build");

        let execution = LaunchPlanBuilder::new()
            .profile(Profile::new("work", "generic", PrivacyPolicy::default()))
            .adapter(TargetAdapter::new(
                "generic",
                CapabilitySet::new(),
                CapabilitySet::new(),
                PrivacyPolicy::default(),
            ))
            .command(vec!["true".to_string()])
            .adapter_policy(policy)
            .build()
            .expect("launch plan should build");

        let host_name = execution
            .env_plan
            .iter()
            .find(|(key, _)| key == "HOSTNAME")
            .map(|(_, value)| value.clone());
        let computer_name = execution
            .env_plan
            .iter()
            .find(|(key, _)| key == "COMPUTERNAME")
            .map(|(_, value)| value.clone());

        assert_eq!(host_name.as_deref(), Some("host-fake"));
        assert_eq!(computer_name.as_deref(), Some("host-fake"));
    }

    #[test]
    fn validate_live_audit_payload_flags_missing_legacy_telemetry_guards() {
        let payload = LiveAuditPayload {
            claude_code_enable_telemetry: None,
            node_use_env_proxy: None,
            do_not_track: Some("1".to_string()),
            otel_sdk_disabled: Some("true".to_string()),
            otel_traces_exporter: None,
            otel_metrics_exporter: None,
            otel_logs_exporter: None,
            sentry_dsn: None,
            disable_error_reporting: None,
            disable_bug_command: None,
            claude_code_disable_nonessential_traffic: None,
            telemetry_disabled: None,
            disable_telemetry: Some("1".to_string()),
            ccp_runtime_hook: Some("/tmp/hook.js".to_string()),
            hostaliases: Some("/tmp/blocked_hosts".to_string()),
            anthropic_base_url: None,
            anthropic_auth_token: None,
            anthropic_api_key: None,
            dns_error_code: Some("ECONNREFUSED".to_string()),
            dns_blocked: true,
        };

        let problems = validate_live_audit_payload(&payload);

        assert!(problems
            .iter()
            .any(|item| item.contains("CLAUDE_CODE_ENABLE_TELEMETRY")));
        assert!(problems
            .iter()
            .any(|item| item.contains("NODE_USE_ENV_PROXY")));
        assert!(problems
            .iter()
            .any(|item| item.contains("OTEL_TRACES_EXPORTER")));
        assert!(problems.iter().any(|item| item.contains("SENTRY_DSN")));
        assert!(problems
            .iter()
            .any(|item| item.contains("DISABLE_ERROR_REPORTING")));
        assert!(problems
            .iter()
            .any(|item| item.contains("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")));
        assert!(problems
            .iter()
            .any(|item| item.contains("TELEMETRY_DISABLED")));
    }
}
