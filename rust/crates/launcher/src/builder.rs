use crate::{env_plan::EnvPlan, session::Session};
use core::{LaunchPlan, LaunchPlanError, Profile, TargetAdapter};
use sidecar::{CreateSessionRequest, SidecarError, SidecarServer};
use std::{fmt, path::PathBuf};

#[derive(Clone, Debug, Default)]
pub struct AdapterLaunchPolicy {
    env_overrides: Vec<(String, String)>,
    env_unsets: Vec<String>,
    runtime_hook_path: Option<PathBuf>,
}

impl AdapterLaunchPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_env_override(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.env_overrides.push((key.into(), value.into()));
        self
    }

    pub fn with_env_unset(mut self, key: impl Into<String>) -> Self {
        self.env_unsets.push(key.into());
        self
    }

    pub fn with_runtime_hook_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.runtime_hook_path = Some(path.into());
        self
    }
}

#[derive(Clone, Debug)]
pub struct LaunchPlanBuilder {
    profile: Option<Profile>,
    adapter: Option<TargetAdapter>,
    command: Option<Vec<String>>,
    env_plan: EnvPlan,
    adapter_policy: AdapterLaunchPolicy,
    session: Option<Session>,
}

impl Default for LaunchPlanBuilder {
    fn default() -> Self {
        LaunchPlanBuilder {
            profile: None,
            adapter: None,
            command: None,
            env_plan: EnvPlan::new(),
            adapter_policy: AdapterLaunchPolicy::new(),
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

    pub fn adapter_policy(mut self, policy: AdapterLaunchPolicy) -> Self {
        self.adapter_policy = policy;
        self
    }

    pub fn session(mut self, session: Session) -> Self {
        self.session = Some(session);
        self
    }

    pub fn build(mut self) -> Result<LaunchPlanExecution, LaunchError> {
        let profile = self.profile.ok_or(LaunchError::MissingProfile)?;
        let adapter = self.adapter.ok_or(LaunchError::MissingAdapter)?;
        let command = self.command.ok_or(LaunchError::MissingCommand)?;
        if command.is_empty() {
            return Err(LaunchError::MissingCommand);
        }

        let plan = LaunchPlan::new(profile, adapter).map_err(LaunchError::Plan)?;
        let adapter_name = plan.adapter_identity().to_string();
        let provided_capabilities = core::CapabilitySet::current_platform_capabilities();
        let missing_capabilities = plan.required_capabilities().difference(&provided_capabilities);
        if !missing_capabilities.is_empty() {
            return Err(LaunchError::MissingRequiredCapabilities {
                adapter_name,
                platform: core::CapabilitySet::current_platform_identity().to_string(),
                missing_capabilities,
            });
        }

        let adapter_name = plan.adapter_identity().to_string();
        let requires_sidecar = requires_sidecar_for_adapter(adapter_name.as_str());
        let session = match self.session {
            Some(session) => normalize_session(session, adapter_name.as_str(), requires_sidecar),
            None => {
                let request = CreateSessionRequest::new(adapter_name.clone(), requires_sidecar);
                let response = SidecarServer::new()
                    .create_session(request)
                    .map_err(LaunchError::Sidecar)?;
                Session::from_metadata(response.metadata().clone())
            }
        };

        self.env_plan.insert("CCP_SESSION_ID", session.id.clone());

        for key in self.adapter_policy.env_unsets {
            self.env_plan.unset(key);
        }

        for (key, value) in self.adapter_policy.env_overrides {
            self.env_plan.insert(key, value);
        }

        if let Some(runtime_hook_path) = self.adapter_policy.runtime_hook_path {
            let runtime_hook_value = runtime_hook_path.display().to_string();
            self.env_plan
                .insert("CCP_RUNTIME_HOOK", runtime_hook_value.clone());
            let existing_node_options = self
                .env_plan
                .latest_value("NODE_OPTIONS")
                .map(str::to_owned)
                .or_else(|| std::env::var("NODE_OPTIONS").ok());
            self.env_plan.insert(
                "NODE_OPTIONS",
                compose_node_options(existing_node_options.as_deref(), runtime_hook_value.as_str()),
            );
        }

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
    MissingRequiredCapabilities {
        adapter_name: String,
        platform: String,
        missing_capabilities: core::CapabilitySet,
    },
    Plan(LaunchPlanError),
    Sidecar(SidecarError),
    Execution(std::io::Error),
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaunchError::MissingProfile => write!(f, "missing profile for launch plan"),
            LaunchError::MissingAdapter => write!(f, "missing adapter for launch plan"),
            LaunchError::MissingCommand => write!(f, "missing command to launch"),
            LaunchError::MissingRequiredCapabilities {
                adapter_name,
                platform,
                missing_capabilities,
            } => write!(
                f,
                "required capability mismatch for adapter `{}` on platform `{}`: missing {}",
                adapter_name,
                platform,
                render_capability_set(missing_capabilities)
            ),
            LaunchError::Plan(err) => write!(f, "{}", err),
            LaunchError::Sidecar(err) => write!(f, "{}", err),
            LaunchError::Execution(err) => write!(f, "failed to execute command: {}", err),
        }
    }
}

impl std::error::Error for LaunchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LaunchError::Plan(err) => Some(err),
            LaunchError::Sidecar(err) => Some(err),
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

fn requires_sidecar_for_adapter(adapter_name: &str) -> bool {
    adapter_name.eq_ignore_ascii_case("claude")
}

fn normalize_session(
    mut session: Session,
    adapter_name: &str,
    requires_sidecar: bool,
) -> Session {
    session.adapter = adapter_name.to_string();
    session.sidecar_required |= requires_sidecar;
    session.protocol_version = sidecar::SIDECAR_PROTOCOL_VERSION;
    session
}

fn compose_node_options(existing: Option<&str>, runtime_hook_path: &str) -> String {
    let preload_flag = format!("--require={}", quote_node_option_value(runtime_hook_path));
    let existing = existing
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default();

    if existing.is_empty() {
        return preload_flag;
    }

    if existing.contains(runtime_hook_path) {
        return existing.to_string();
    }

    format!("{existing} {preload_flag}")
}

fn quote_node_option_value(value: &str) -> String {
    if !value.contains(char::is_whitespace) && !value.contains('"') {
        return value.to_string();
    }

    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn render_capability_set(capabilities: &core::CapabilitySet) -> String {
    capabilities
        .iter()
        .map(|value| value.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
