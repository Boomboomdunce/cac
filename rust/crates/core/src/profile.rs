use crate::{PrivacyPolicy, RedactedPrivacyPolicy};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ClaudeProviderConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ClaudeProfileConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ClaudeProviderConfig>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RedactedClaudeProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RedactedClaudeProfileConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<RedactedClaudeProviderConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub adapter: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<ClaudeProfileConfig>,
    pub policy: PrivacyPolicy,
}

#[derive(Clone, Debug, Serialize)]
pub struct RedactedProfile<'a> {
    pub name: &'a str,
    pub adapter: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude: Option<RedactedClaudeProfileConfig>,
    pub policy: RedactedPrivacyPolicy<'a>,
}

impl Profile {
    pub fn new(name: impl Into<String>, adapter: impl Into<String>, policy: PrivacyPolicy) -> Self {
        Profile {
            name: name.into(),
            adapter: adapter.into(),
            claude: None,
            policy,
        }
    }

    pub fn with_claude_provider(mut self, provider: ClaudeProviderConfig) -> Self {
        self.claude = Some(ClaudeProfileConfig {
            provider: Some(provider),
        });
        self
    }

    pub fn redacted(&self) -> RedactedProfile<'_> {
        RedactedProfile {
            name: &self.name,
            adapter: &self.adapter,
            claude: self
                .claude
                .as_ref()
                .map(|claude| RedactedClaudeProfileConfig {
                    provider: claude.provider.as_ref().map(|provider| {
                        RedactedClaudeProviderConfig {
                            base_url: provider.base_url.clone(),
                            auth_token: provider.auth_token.as_ref().map(|_| "***".to_string()),
                            api_key: provider.api_key.as_ref().map(|_| "***".to_string()),
                        }
                    }),
                }),
            policy: self.policy.redacted(),
        }
    }
}
