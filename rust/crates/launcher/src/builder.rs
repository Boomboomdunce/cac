use crate::{env_plan::EnvPlan, session::Session};
use core::{LaunchPlan, LaunchPlanError, Profile, TargetAdapter};
use std::fmt;

#[derive(Clone, Debug)]
pub struct LaunchPlanBuilder {
    profile: Option<Profile>,
    adapter: Option<TargetAdapter>,
    command: Option<Vec<String>>,
    env_plan: EnvPlan,
    session: Option<Session>,
}

impl Default for LaunchPlanBuilder {
    fn default() -> Self {
        LaunchPlanBuilder {
            profile: None,
            adapter: None,
            command: None,
            env_plan: EnvPlan::new(),
            session: None,
        }
    }
}

impl LaunchPlanBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn profile(mut self, profile: Profile) -> Self {
        self.profile = Some(profile);
        self
    }

    pub fn adapter(mut self, adapter: TargetAdapter) -> Self {
        self.adapter = Some(adapter);
        self
    }

    pub fn command(mut self, command: Vec<String>) -> Self {
        self.command = Some(command);
        self
    }

    pub fn env_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_plan.insert(key, value);
        self
    }

    pub fn env_plan(mut self, env_plan: EnvPlan) -> Self {
        self.env_plan = env_plan;
        self
    }

    pub fn session(mut self, session: Session) -> Self {
        self.session = Some(session);
        self
    }

    pub fn build(self) -> Result<LaunchPlanExecution, LaunchError> {
        let profile = self.profile.ok_or(LaunchError::MissingProfile)?;
        let adapter = self.adapter.ok_or(LaunchError::MissingAdapter)?;
        let command = self.command.ok_or(LaunchError::MissingCommand)?;
        if command.is_empty() {
            return Err(LaunchError::MissingCommand);
        }

        let plan = LaunchPlan::new(profile, adapter).map_err(LaunchError::Plan)?;
        let session = self.session.unwrap_or_else(Session::placeholder);

        Ok(LaunchPlanExecution {
            plan,
            command,
            env_plan: self.env_plan,
            session,
        })
    }
}

#[derive(Clone, Debug)]
pub struct LaunchPlanExecution {
    pub plan: LaunchPlan,
    pub command: Vec<String>,
    pub env_plan: EnvPlan,
    pub session: Session,
}

#[derive(Debug)]
pub enum LaunchError {
    MissingProfile,
    MissingAdapter,
    MissingCommand,
    Plan(LaunchPlanError),
    Execution(std::io::Error),
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaunchError::MissingProfile => write!(f, "missing profile for launch plan"),
            LaunchError::MissingAdapter => write!(f, "missing adapter for launch plan"),
            LaunchError::MissingCommand => write!(f, "missing command to launch"),
            LaunchError::Plan(err) => write!(f, "{}", err),
            LaunchError::Execution(err) => write!(f, "failed to execute command: {}", err),
        }
    }
}

impl std::error::Error for LaunchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LaunchError::Plan(err) => Some(err),
            LaunchError::Execution(err) => Some(err),
            _ => None,
        }
    }
}

impl From<LaunchPlanError> for LaunchError {
    fn from(err: LaunchPlanError) -> Self {
        LaunchError::Plan(err)
    }
}

impl From<std::io::Error> for LaunchError {
    fn from(err: std::io::Error) -> Self {
        LaunchError::Execution(err)
    }
}
