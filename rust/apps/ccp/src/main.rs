use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use core::{PrivacyPolicy, Profile};
use std::{env, path::PathBuf};
use store::{ProfileStore, StateLayout};

#[derive(Parser)]
#[command(name = "ccp", about = "Command Privacy Proxy", version, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Profile(ProfileGroup),
    Run,
    Doctor,
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
        Some(Commands::Run) => {
            println!("`run` is not implemented yet");
            Ok(())
        }
        Some(Commands::Doctor) => {
            println!("`doctor` is not implemented yet");
            Ok(())
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

fn state_root() -> Result<PathBuf, std::io::Error> {
    if let Some(root) = env::var_os("CCP_STATE_ROOT") {
        Ok(PathBuf::from(root))
    } else {
        env::current_dir().map(|cwd| cwd.join("ccp-state"))
    }
}
