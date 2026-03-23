use sidecar::SidecarSessionMetadata;

#[derive(Clone, Debug)]
pub struct Session {
    pub id: String,
    pub adapter: String,
    pub sidecar_required: bool,
    pub protocol_version: u32,
}

impl Session {
    pub fn placeholder() -> Self {
        Session {
            id: "placeholder-session".into(),
            adapter: "generic".into(),
            sidecar_required: false,
            protocol_version: sidecar::SIDECAR_PROTOCOL_VERSION,
        }
    }

    pub fn from_metadata(metadata: SidecarSessionMetadata) -> Self {
        Session {
            id: metadata.session_id,
            adapter: metadata.adapter,
            sidecar_required: metadata.requires_sidecar,
            protocol_version: metadata.protocol_version,
        }
    }
}
