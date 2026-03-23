pub mod server;
pub mod session;

pub use sidecar_proto::{
    CreateSessionRequest, CreateSessionResponse, SidecarSessionMetadata, SIDECAR_PROTOCOL_VERSION,
};
pub use server::{SidecarError, SidecarServer};
pub use session::{PublicSessionAuditEvent, SessionAuditEvent, SidecarSession};
