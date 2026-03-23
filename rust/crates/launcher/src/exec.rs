use crate::builder::{LaunchError, LaunchPlanExecution};
use std::process::{Command, ExitStatus};

pub fn execute(execution: &LaunchPlanExecution) -> Result<ExitStatus, LaunchError> {
    let mut command_iter = execution.command.iter();
    let program = match command_iter.next() {
        Some(cmd) => cmd,
        None => return Err(LaunchError::MissingCommand),
    };

    let mut cmd = Command::new(program);
    cmd.args(command_iter);
    for key in execution.env_plan.removals() {
        cmd.env_remove(key);
    }
    for (key, value) in execution.env_plan.iter() {
        cmd.env(key, value);
    }

    cmd.status().map_err(LaunchError::Execution)
}
