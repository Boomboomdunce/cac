use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use core::{CapabilitySet, PrivacyPolicy, Profile, TargetAdapter};
use doctor::DoctorConfig;
use launcher::{builder::LaunchPlanBuilder, exec};
use std::{env, path::PathBuf, process};
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
    let store = ProfileStore::new(layout);
    let profile = store.load_profile(&profile).context("loading profile")?;

    let adapter = TargetAdapter::new(
        profile.adapter.clone(),
        CapabilitySet::new(),
        CapabilitySet::new(),
        PrivacyPolicy::default(),
    );

    let plan = LaunchPlanBuilder::new()
        .profile(profile)
        .adapter(adapter)
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
