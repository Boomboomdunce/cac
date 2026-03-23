use core::CapabilitySet;

pub const PLATFORM_IDENTITY: &str = "windows";

pub fn platform_identity() -> &'static str {
    PLATFORM_IDENTITY
}

pub fn provided_capabilities() -> CapabilitySet {
    CapabilitySet::from(["node_preload", "sidecar"])
}
