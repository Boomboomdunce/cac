use crate::{adapter::TargetAdapter, capability::CapabilitySet, policy::PrivacyPolicy, profile::Profile};
use std::fmt;

#[derive(Clone, Debug)]
pub struct LaunchPlan {
    profile: Profile,
    adapter: TargetAdapter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchPlanError {
    AdapterMismatch {
        profile_adapter: String,
        adapter_name: String,
    },
}

impl fmt::Display for LaunchPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaunchPlanError::AdapterMismatch {
                profile_adapter,
                adapter_name,
            } => write!(
                f,
                "profile adapter `{}` does not match target adapter `{}`",
                profile_adapter, adapter_name
            ),
        }
    }
}

impl std::error::Error for LaunchPlanError {}

impl LaunchPlan {
    pub fn new(profile: Profile, adapter: TargetAdapter) -> Result<Self, LaunchPlanError> {
        let profile_adapter = profile.adapter.clone();
        let adapter_name = adapter.name.clone();

        if profile_adapter != adapter_name {
            return Err(LaunchPlanError::AdapterMismatch {
                profile_adapter,
                adapter_name,
            });
        }

        Ok(LaunchPlan { profile, adapter })
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn adapter(&self) -> &TargetAdapter {
        &self.adapter
    }

    pub fn policy(&self) -> PrivacyPolicy {
        self.profile
            .policy
            .clone()
            .merge(self.adapter.policy.clone())
    }

    pub fn required_capabilities(&self) -> &CapabilitySet {
        &self.adapter.required_capabilities
    }

    pub fn preferred_capabilities(&self) -> &CapabilitySet {
        &self.adapter.preferred_capabilities
    }

    pub fn adapter_identity(&self) -> &str {
        &self.adapter.name
    }

    pub fn satisfies_required_capabilities(&self, provided: &CapabilitySet) -> bool {
        self.required_capabilities().is_subset_of(provided)
    }

    pub fn effective_capabilities(&self) -> CapabilitySet {
        self.required_capabilities()
            .union(self.preferred_capabilities())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySet;
    use crate::policy::PrivacyPolicy;

    #[test]
    fn new_rejects_adapter_name_mismatch() {
        let profile = Profile::new("work", "claude", PrivacyPolicy::default());
        let adapter = TargetAdapter::new(
            "node",
            CapabilitySet::new(),
            CapabilitySet::new(),
            PrivacyPolicy::default(),
        );

        let err = LaunchPlan::new(profile, adapter).unwrap_err();

        assert_eq!(
            err,
            LaunchPlanError::AdapterMismatch {
                profile_adapter: "claude".to_string(),
                adapter_name: "node".to_string(),
            }
        );
    }
}
