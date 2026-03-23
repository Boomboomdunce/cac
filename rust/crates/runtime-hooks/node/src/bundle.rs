#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeHookBundle {
    id: &'static str,
    relative_path: &'static str,
    contents: &'static str,
}

impl RuntimeHookBundle {
    pub const fn new(
        id: &'static str,
        relative_path: &'static str,
        contents: &'static str,
    ) -> Self {
        Self {
            id,
            relative_path,
            contents,
        }
    }

    pub const fn id(&self) -> &'static str {
        self.id
    }

    pub const fn relative_path(&self) -> &'static str {
        self.relative_path
    }

    pub const fn contents(&self) -> &'static str {
        self.contents
    }
}

const CLAUDE_PRELOAD: &str = include_str!("../../../../hooks/node/claude-preload.js");

pub fn claude_preload_bundle() -> RuntimeHookBundle {
    RuntimeHookBundle::new(
        "claude-node-preload",
        "hooks/node/claude-preload.js",
        CLAUDE_PRELOAD,
    )
}
