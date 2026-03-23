use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use claude_adapter::{claude_adapter, ADAPTER_NAME};
use core::{CapabilitySet, PrivacyPolicy, Profile, TargetAdapter};
use doctor::DoctorConfig;
use launcher::{
    builder::{AdapterLaunchPolicy, LaunchPlanBuilder},
    exec,
};
use std::{env, fs, path::PathBuf, process};
use store::{ProfileStore, StateLayout};

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
    Profile(ProfileGroup),
    Run(RunCommand),
    Doctor(DoctorCommand),
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
    profile: String,
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

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
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
            let report = doctor::run(DoctorConfig::new(root.clone(), cmd.profile));

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
        None => Ok(()),
    }
}

fn profile_command_handler(store: &ProfileStore, command: ProfileCommand) -> Result<()> {
    match command {
        ProfileCommand::Create { name, adapter } => {
            let profile = Profile::new(name.clone(), adapter, PrivacyPolicy::default());
            let saved = store.save_profile(&profile).context("saving profile")?;
            println!("{}", saved.display());
            Ok(())
        }
        ProfileCommand::Show { name } => {
            let profile = store.load_profile(&name).context("loading profile")?;
            let json = serde_json::to_string_pretty(&profile)?;
            println!("{json}");
            Ok(())
        }
        ProfileCommand::List => {
            let profiles = store.list_profiles().context("listing profiles")?;
            for profile in profiles {
                println!("{} ({})", profile.name, profile.adapter);
            }
            Ok(())
        }
    }
}

fn run_command_handler(RunCommand { profile, command }: RunCommand) -> Result<process::ExitStatus> {
    let layout = StateLayout::new(state_root()?).context("initializing CCP state layout")?;
    let store = ProfileStore::new(layout.clone());
    let profile = store.load_profile(&profile).context("loading profile")?;
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
    if let Some(root) = env::var_os("CCP_STATE_ROOT") {
        Ok(PathBuf::from(root))
    } else {
        env::current_dir().map(|cwd| cwd.join("ccp-state"))
    }
}

fn resolve_adapter_policy(
    profile: &Profile,
    layout: &StateLayout,
) -> Result<(TargetAdapter, AdapterLaunchPolicy)> {
    if profile.adapter.eq_ignore_ascii_case(ADAPTER_NAME) {
        let adapter = claude_adapter();
        let runtime_hook = materialize_runtime_hook(
            layout.hooks_dir().join("claude-preload.js"),
            adapter.runtime_hook_bundle().contents(),
        )
        .context("materializing Claude runtime hook")?;

        let mut policy = AdapterLaunchPolicy::new().with_runtime_hook_path(runtime_hook);
        for (key, value) in adapter.environment_overrides() {
            policy = policy.with_env_override(key.clone(), value.clone());
        }
        for key in adapter.environment_unsets() {
            policy = policy.with_env_unset(key.clone());
        }

        Ok((adapter.target_adapter().clone(), policy))
    } else {
        Ok((
            TargetAdapter::new(
                profile.adapter.clone(),
                CapabilitySet::new(),
                CapabilitySet::new(),
                PrivacyPolicy::default(),
            ),
            AdapterLaunchPolicy::new(),
        ))
    }
}

fn materialize_runtime_hook(path: PathBuf, contents: &str) -> Result<PathBuf> {
    fs::write(&path, contents).with_context(|| format!("writing runtime hook to {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&path, permissions)?;
    }
    Ok(path)
}
