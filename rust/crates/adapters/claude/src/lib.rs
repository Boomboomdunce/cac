mod env;
mod policy;

use core::{CapabilitySet, TargetAdapter};
use runtime_hooks_node::{claude_preload_bundle, RuntimeHookBundle};
use std::collections::{BTreeMap, BTreeSet};

pub const ADAPTER_NAME: &str = "claude";
pub const CAPABILITY_NODE_PRELOAD: &str = "node_preload";
pub const CAPABILITY_SIDECAR: &str = "sidecar";

#[derive(Clone, Debug)]
pub struct ClaudeAdapter {
    target_adapter: TargetAdapter,
    env_overrides: BTreeMap<String, String>,
    env_unsets: BTreeSet<String>,
    runtime_hook: RuntimeHookBundle,
    sidecar_required: bool,
}

impl ClaudeAdapter {
    pub fn new() -> Self {
        let required_capabilities =
            CapabilitySet::from([CAPABILITY_NODE_PRELOAD, CAPABILITY_SIDECAR]);
        let target_adapter = TargetAdapter::new(
            ADAPTER_NAME,
            required_capabilities,
            CapabilitySet::new(),
            policy::claude_policy(),
        );

        Self {
            target_adapter,
            env_overrides: env::claude_env_overrides(),
            env_unsets: env::claude_env_unsets(),
            runtime_hook: claude_preload_bundle(),
            sidecar_required: true,
        }
    }

    pub fn blocked_hosts(&self) -> &BTreeSet<String> {
        self.target_adapter.policy.blocked_hosts()
    }

    pub fn environment_overrides(&self) -> &BTreeMap<String, String> {
        &self.env_overrides
    }

    pub fn environment_unsets(&self) -> &BTreeSet<String> {
        &self.env_unsets
    }

    pub fn required_capabilities(&self) -> &CapabilitySet {
        &self.target_adapter.required_capabilities
    }

    pub fn target_adapter(&self) -> &TargetAdapter {
        &self.target_adapter
    }

    pub fn runtime_hook_bundle(&self) -> RuntimeHookBundle {
        self.runtime_hook
    }

    pub fn sidecar_required(&self) -> bool {
        self.sidecar_required
    }
}

impl Default for ClaudeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn claude_adapter() -> ClaudeAdapter {
    ClaudeAdapter::new()
}
