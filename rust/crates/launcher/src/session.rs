#[derive(Clone, Debug)]
pub struct Session {
    pub id: String,
    pub sidecar_required: bool,
}

impl Session {
    pub fn placeholder() -> Self {
        Session {
            id: "placeholder-session".into(),
            sidecar_required: false,
        }
    }
}
