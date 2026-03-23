use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ccp", about = "Command Privacy Proxy", version, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Profile,
    Run,
    Doctor,
}

fn main() {
    let _ = Cli::parse();
}
