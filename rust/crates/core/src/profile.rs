use crate::{PrivacyPolicy, RedactedPrivacyPolicy};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub adapter: String,
    pub policy: PrivacyPolicy,
}

#[derive(Clone, Debug, Serialize)]
pub struct RedactedProfile<'a> {
    pub name: &'a str,
    pub adapter: &'a str,
    pub policy: RedactedPrivacyPolicy<'a>,
}

impl Profile {
    pub fn new(name: impl Into<String>, adapter: impl Into<String>, policy: PrivacyPolicy) -> Self {
        Profile {
            name: name.into(),
            adapter: adapter.into(),
            policy,
        }
    }

    pub fn redacted(&self) -> RedactedProfile<'_> {
        RedactedProfile {
            name: &self.name,
            adapter: &self.adapter,
            policy: self.policy.redacted(),
        }
    }
}
