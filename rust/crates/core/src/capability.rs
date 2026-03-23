use std::collections::BTreeSet;
use std::iter::FromIterator;

#[cfg(target_os = "macos")]
const CURRENT_PLATFORM_IDENTITY: &str = "macos";
#[cfg(target_os = "linux")]
const CURRENT_PLATFORM_IDENTITY: &str = "linux";
#[cfg(target_os = "windows")]
const CURRENT_PLATFORM_IDENTITY: &str = "windows";
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
const CURRENT_PLATFORM_IDENTITY: &str = "unknown";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformDoctorCheck {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

impl PlatformDoctorCheck {
    pub fn ok(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ok: true,
            message: message.into(),
        }
    }

    pub fn error(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ok: false,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySet {
    inner: BTreeSet<String>,
}

impl CapabilitySet {
    pub fn new() -> Self {
        Self {
            inner: BTreeSet::new(),
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        self.inner.contains(key)
    }

    pub fn insert(&mut self, key: impl Into<String>) -> bool {
        self.inner.insert(key.into())
    }

    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.inner.is_subset(&other.inner)
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn difference(&self, other: &Self) -> Self {
        CapabilitySet {
            inner: self
                .inner
                .difference(&other.inner)
                .cloned()
                .collect(),
        }
    }

    pub fn union(&self, other: &Self) -> Self {
        CapabilitySet {
            inner: self.inner.union(&other.inner).cloned().collect(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.inner.iter()
    }

    pub fn current_platform_identity() -> &'static str {
        CURRENT_PLATFORM_IDENTITY
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    pub fn current_platform_capabilities() -> Self {
        CapabilitySet::from(["node_preload", "sidecar"])
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    pub fn current_platform_capabilities() -> Self {
        CapabilitySet::new()
    }
}

impl<T> From<T> for CapabilitySet
where
    T: IntoIterator,
    T::Item: Into<String>,
{
    fn from(iter: T) -> Self {
        CapabilitySet {
            inner: BTreeSet::from_iter(iter.into_iter().map(Into::into)),
        }
    }
}
