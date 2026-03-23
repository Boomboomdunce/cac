mod adapter;
mod capability;
mod launch_plan;
mod policy;
mod profile;

pub use adapter::TargetAdapter;
pub use capability::{CapabilitySet, PlatformDoctorCheck};
pub use launch_plan::{LaunchPlan, LaunchPlanError};
pub use policy::{redact_proxy_url, redact_sensitive_text, PrivacyPolicy, RedactedPrivacyPolicy};
pub use profile::{Profile, RedactedProfile};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_capability_mismatch_is_rejected() {
        let required = CapabilitySet::from(["node_preload", "proxy"]);
        let provided = CapabilitySet::from(["proxy"]);
        assert!(!required.is_subset_of(&provided));
    }

    #[test]
    fn adapter_policy_overrides_profile_defaults() {
        let merged = PrivacyPolicy::default()
            .with_blocked_host("example.com")
            .merge(
                PrivacyPolicy::default()
                    .with_blocked_host("statsig.anthropic.com"),
            );
        assert!(merged
            .blocked_hosts()
            .contains(&"statsig.anthropic.com".to_string()));
    }
}
