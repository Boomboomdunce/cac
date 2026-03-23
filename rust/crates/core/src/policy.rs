use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PrivacyPolicy {
    #[serde(default)]
    blocked_hosts: BTreeSet<String>,
}

impl PrivacyPolicy {
    pub fn new() -> Self {
        Self {
            blocked_hosts: BTreeSet::new(),
        }
    }

    pub fn with_blocked_host(mut self, host: impl Into<String>) -> Self {
        self.blocked_hosts.insert(host.into());
        self
    }

    pub fn blocked_hosts(&self) -> &BTreeSet<String> {
        &self.blocked_hosts
    }

    pub fn merge(mut self, other: PrivacyPolicy) -> PrivacyPolicy {
        self.blocked_hosts = self
            .blocked_hosts
            .union(&other.blocked_hosts)
            .cloned()
            .collect();
        self
    }
}
