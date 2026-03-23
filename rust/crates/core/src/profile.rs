use crate::PrivacyPolicy;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub adapter: String,
    pub policy: PrivacyPolicy,
}

impl Profile {
    pub fn new(
        name: impl Into<String>,
        adapter: impl Into<String>,
        policy: PrivacyPolicy,
    ) -> Self {
        Profile {
            name: name.into(),
            adapter: adapter.into(),
            policy,
        }
    }
}
