use core::{CapabilitySet, PlatformDoctorCheck};

pub const PLATFORM_IDENTITY: &str = "linux";

pub fn platform_identity() -> &'static str {
    PLATFORM_IDENTITY
}

pub fn provided_capabilities() -> CapabilitySet {
    CapabilitySet::from(["node_preload", "sidecar"])
}

pub fn doctor_checks(required: &CapabilitySet) -> Vec<PlatformDoctorCheck> {
    let missing = required.difference(&provided_capabilities());
    if missing.is_empty() {
        vec![PlatformDoctorCheck::ok(
            "platform capability support",
            "platform 'linux' provides required capabilities",
        )]
    } else {
        vec![PlatformDoctorCheck::error(
            "platform capability support",
            format!(
                "platform 'linux' is missing required capabilities: {}",
                missing.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        )]
    }
}
