use core::redact_proxy_url;
use serde::Serialize;
use sidecar_proto::SidecarSessionMetadata;

#[derive(Clone, Debug)]
pub struct SidecarSession {
    metadata: SidecarSessionMetadata,
}

impl SidecarSession {
    pub fn new(metadata: SidecarSessionMetadata) -> Self {
        SidecarSession { metadata }
    }

    pub fn metadata(&self) -> &SidecarSessionMetadata {
        &self.metadata
    }

    pub fn launch_started_audit_event(&self) -> SessionAuditEvent {
        SessionAuditEvent::launch_started(self.metadata.clone())
    }
}

#[derive(Clone, Debug)]
pub struct SessionAuditEvent {
    event: String,
    session_id: String,
    adapter: String,
    requires_sidecar: bool,
    proxy_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PublicSessionAuditEvent {
    pub event: String,
    pub session_id: String,
    pub adapter: String,
    pub requires_sidecar: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
}

impl SessionAuditEvent {
    pub fn launch_started(metadata: SidecarSessionMetadata) -> Self {
        Self {
            event: "launch_started".to_string(),
            session_id: metadata.session_id,
            adapter: metadata.adapter,
            requires_sidecar: metadata.requires_sidecar,
            proxy_url: None,
        }
    }

    pub fn with_proxy_url(mut self, proxy_url: impl Into<String>) -> Self {
        self.proxy_url = Some(proxy_url.into());
        self
    }

    pub fn public_event(&self) -> PublicSessionAuditEvent {
        PublicSessionAuditEvent {
            event: self.event.clone(),
            session_id: self.session_id.clone(),
            adapter: self.adapter.clone(),
            requires_sidecar: self.requires_sidecar,
            proxy_url: self.proxy_url.as_deref().map(redact_proxy_url),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_event_report_redacts_proxy_credentials() {
        let session = SidecarSession::new(SidecarSessionMetadata::new("claude", true, "session-1"));
        let event = session
            .launch_started_audit_event()
            .with_proxy_url("https://alice:super-secret@proxy.example:8443");

        let report = event.public_event();

        assert_eq!(report.session_id, "session-1");
        assert_eq!(report.adapter, "claude");
        assert_eq!(
            report.proxy_url.as_deref(),
            Some("https://alice:***@proxy.example:8443")
        );
        assert!(!report
            .proxy_url
            .as_deref()
            .unwrap()
            .contains("super-secret"));
    }
}
