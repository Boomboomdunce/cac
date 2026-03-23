use crate::{capability::CapabilitySet, policy::PrivacyPolicy};

#[derive(Clone, Debug)]
pub struct TargetAdapter {
    pub name: String,
    pub required_capabilities: CapabilitySet,
    pub preferred_capabilities: CapabilitySet,
    pub policy: PrivacyPolicy,
}

impl TargetAdapter {
    pub fn new(
        name: impl Into<String>,
        required_capabilities: CapabilitySet,
        preferred_capabilities: CapabilitySet,
        policy: PrivacyPolicy,
    ) -> Self {
        TargetAdapter {
            name: name.into(),
            required_capabilities,
            preferred_capabilities,
            policy,
        }
    }
}
