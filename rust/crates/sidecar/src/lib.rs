pub mod capture;
pub mod egress;
pub mod proxy;
pub mod server;
pub mod session;

pub use capture::{CaptureBuffer, CapturedRequest};
pub use egress::detect_egress_ip;
pub use proxy::{CaptureProxy, CaptureProxyConfig};
pub use server::{SidecarError, SidecarServer};
pub use session::{PublicSessionAuditEvent, SessionAuditEvent, SidecarSession};
pub use sidecar_proto::{
    CreateSessionRequest, CreateSessionResponse, SidecarSessionMetadata, SIDECAR_PROTOCOL_VERSION,
};
