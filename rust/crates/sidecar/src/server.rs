use crate::session::SidecarSession;
use sidecar_proto::{
    CreateSessionRequest, CreateSessionResponse, SidecarSessionMetadata, SIDECAR_PROTOCOL_VERSION,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidecarError {
    ProtocolVersionMismatch { expected: u32, actual: u32 },
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SidecarError::ProtocolVersionMismatch { expected, actual } => write!(
                f,
                "sidecar protocol version mismatch: expected {}, got {}",
                expected, actual
            ),
        }
    }
}

impl std::error::Error for SidecarError {}

pub struct SidecarServer;

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

impl Default for SidecarServer {
    fn default() -> Self {
        Self::new()
    }
}

impl SidecarServer {
    pub fn new() -> Self {
        SidecarServer
    }

    pub fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> Result<CreateSessionResponse, SidecarError> {
        let CreateSessionRequest {
            adapter,
            requires_sidecar,
            protocol_version,
        } = request;

        if protocol_version != SIDECAR_PROTOCOL_VERSION {
            return Err(SidecarError::ProtocolVersionMismatch {
                expected: SIDECAR_PROTOCOL_VERSION,
                actual: protocol_version,
            });
        }
        let session_id = format!(
            "sidecar-{}-{}-{}",
            adapter,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default(),
            SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
        );

        let metadata = SidecarSessionMetadata::new(adapter.clone(), requires_sidecar, session_id);
        let session = SidecarSession::new(metadata.clone());
        let _ = session; // keep session struct for future extension

        Ok(CreateSessionResponse::new(metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn create_session_rejects_protocol_version_mismatch() {
        let mut request = CreateSessionRequest::new("claude", true);
        request.protocol_version += 1;

        let err = SidecarServer::new()
            .create_session(request)
            .expect_err("expected protocol mismatch to be rejected");

        assert_eq!(
            err,
            SidecarError::ProtocolVersionMismatch {
                expected: SIDECAR_PROTOCOL_VERSION,
                actual: SIDECAR_PROTOCOL_VERSION + 1,
            }
        );
    }

    #[test]
    fn create_session_generates_distinct_ids_for_burst_requests() {
        let server = SidecarServer::new();
        let mut ids = BTreeSet::new();

        for _ in 0..512 {
            let response = server
                .create_session(CreateSessionRequest::new("claude", true))
                .expect("session creation should succeed");
            let inserted = ids.insert(response.metadata().session_id.clone());
            assert!(inserted, "duplicate session id generated");
        }
    }
}
