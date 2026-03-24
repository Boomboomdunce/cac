use serde::Serialize;
use serde::{Deserialize, Serializer};
use std::collections::BTreeSet;
use std::fmt;

#[derive(Clone, Default, Deserialize)]
pub struct PrivacyPolicy {
    #[serde(default)]
    blocked_hosts: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proxy_url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RedactedPrivacyPolicy<'a> {
    blocked_hosts: &'a BTreeSet<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_url: Option<String>,
}

impl PrivacyPolicy {
    pub fn new() -> Self {
        Self {
            blocked_hosts: BTreeSet::new(),
            proxy_url: None,
        }
    }

    pub fn with_blocked_host(mut self, host: impl Into<String>) -> Self {
        self.blocked_hosts.insert(host.into());
        self
    }

    pub fn with_proxy_url(mut self, proxy_url: impl Into<String>) -> Self {
        self.proxy_url = Some(proxy_url.into());
        self
    }

    pub fn blocked_hosts(&self) -> &BTreeSet<String> {
        &self.blocked_hosts
    }

    pub fn proxy_url(&self) -> Option<&str> {
        self.proxy_url.as_deref()
    }

    pub fn redacted(&self) -> RedactedPrivacyPolicy<'_> {
        RedactedPrivacyPolicy {
            blocked_hosts: &self.blocked_hosts,
            proxy_url: self.proxy_url.as_deref().map(redact_proxy_url),
        }
    }

    pub fn merge(mut self, other: PrivacyPolicy) -> PrivacyPolicy {
        self.blocked_hosts = self
            .blocked_hosts
            .union(&other.blocked_hosts)
            .cloned()
            .collect();
        self.proxy_url = other.proxy_url.or(self.proxy_url);
        self
    }
}

impl Serialize for PrivacyPolicy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct SerializablePrivacyPolicy<'a> {
            blocked_hosts: &'a BTreeSet<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            proxy_url: Option<&'a str>,
        }

        SerializablePrivacyPolicy {
            blocked_hosts: &self.blocked_hosts,
            proxy_url: self.proxy_url.as_deref(),
        }
        .serialize(serializer)
    }
}

impl fmt::Debug for PrivacyPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("PrivacyPolicy");
        debug.field("blocked_hosts", &self.blocked_hosts);
        debug.field(
            "proxy_url",
            &self.proxy_url.as_deref().map(redact_proxy_url),
        );
        debug.finish()
    }
}

pub fn redact_sensitive_text(raw: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0;

    while let Some(start) = find_next_url(&raw[cursor..]) {
        let absolute_start = cursor + start;
        output.push_str(&raw[cursor..absolute_start]);

        let remaining = &raw[absolute_start..];
        let end = remaining
            .find(char::is_whitespace)
            .unwrap_or(remaining.len());
        let candidate = &remaining[..end];
        let (url_token, trailing) = split_trailing_punctuation(candidate);
        output.push_str(&redact_proxy_url(url_token));
        output.push_str(trailing);
        cursor = absolute_start + end;
    }

    output.push_str(&raw[cursor..]);
    output
}

pub fn redact_proxy_url(raw: &str) -> String {
    let Some(scheme_end) = raw.find("://").map(|index| index + 3) else {
        return raw.to_string();
    };

    let authority_end = raw[scheme_end..]
        .find(['/', '?', '#'])
        .map(|index| scheme_end + index)
        .unwrap_or(raw.len());
    let authority = &raw[scheme_end..authority_end];
    let Some(userinfo_end) = authority.rfind('@') else {
        return raw.to_string();
    };
    let userinfo = &authority[..userinfo_end];
    let host = &authority[userinfo_end + 1..];
    let Some(password_start) = userinfo.find(':') else {
        return raw.to_string();
    };
    let username = &userinfo[..password_start];

    format!(
        "{}{}:{}@{}{}",
        &raw[..scheme_end],
        username,
        "***",
        host,
        &raw[authority_end..]
    )
}

pub fn proxy_host_port(raw: &str) -> Option<String> {
    let scheme_end = raw.find("://").map(|index| index + 3)?;

    let authority_end = raw[scheme_end..]
        .find(['/', '?', '#'])
        .map(|index| scheme_end + index)
        .unwrap_or(raw.len());
    let authority = &raw[scheme_end..authority_end];
    let host_port = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);

    if host_port.is_empty() {
        None
    } else {
        Some(host_port.to_string())
    }
}

fn find_next_url(raw: &str) -> Option<usize> {
    ["https://", "http://", "socks5://", "socks5h://"]
        .iter()
        .filter_map(|scheme| raw.find(scheme))
        .min()
}

fn split_trailing_punctuation(raw: &str) -> (&str, &str) {
    let split_at = raw.trim_end_matches([')', ']', '}', ',', ';', '.']).len();
    (&raw[..split_at], &raw[split_at..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_proxy_url_masks_password_only() {
        assert_eq!(
            redact_proxy_url("https://alice:secret@proxy.example:8443"),
            "https://alice:***@proxy.example:8443"
        );
    }

    #[test]
    fn redact_sensitive_text_rewrites_embedded_proxy_urls() {
        let rendered = redact_sensitive_text(
            "using proxy https://alice:secret@proxy.example:8443 for outbound traffic",
        );

        assert!(rendered.contains("https://alice:***@proxy.example:8443"));
        assert!(!rendered.contains("secret"));
    }

    #[test]
    fn proxy_host_port_strips_scheme_and_credentials() {
        assert_eq!(
            proxy_host_port("https://alice:secret@proxy.example:8443"),
            Some("proxy.example:8443".to_string())
        );
    }
}
