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
}
