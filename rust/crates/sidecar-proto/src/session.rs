use serde::{Deserialize, Serialize};

pub const SIDECAR_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SidecarSessionMetadata {
    pub session_id: String,
    pub adapter: String,
    pub requires_sidecar: bool,
    pub protocol_version: u32,
}

impl SidecarSessionMetadata {
    pub fn new(
        adapter: impl Into<String>,
        requires_sidecar: bool,
        session_id: impl Into<String>,
    ) -> Self {
        SidecarSessionMetadata {
            session_id: session_id.into(),
            adapter: adapter.into(),
            requires_sidecar,
            protocol_version: SIDECAR_PROTOCOL_VERSION,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub adapter: String,
    pub requires_sidecar: bool,
    pub protocol_version: u32,
}

impl CreateSessionRequest {
    pub fn new(adapter: impl Into<String>, requires_sidecar: bool) -> Self {
        CreateSessionRequest {
            adapter: adapter.into(),
            requires_sidecar,
            protocol_version: SIDECAR_PROTOCOL_VERSION,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateSessionResponse {
    pub metadata: SidecarSessionMetadata,
}

impl CreateSessionResponse {
    pub fn new(metadata: SidecarSessionMetadata) -> Self {
        CreateSessionResponse { metadata }
    }

    pub fn metadata(&self) -> &SidecarSessionMetadata {
        &self.metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_wraps_metadata() {
        let metadata = SidecarSessionMetadata::new("claude", true, "session-1");
        let response = CreateSessionResponse::new(metadata.clone());
        assert_eq!(response.metadata().adapter, "claude");
        assert_eq!(response.metadata().protocol_version, SIDECAR_PROTOCOL_VERSION);
        assert!(response.metadata().requires_sidecar);
    }
}
