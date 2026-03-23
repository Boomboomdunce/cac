use serde::Serialize;
use std::fmt;

#[derive(Clone, Debug, Serialize)]
pub struct DoctorReport {
    pub ok: bool,
    pub checks: Vec<CheckResult>,
}

impl DoctorReport {
    pub fn new() -> Self {
        DoctorReport {
            ok: true,
            checks: Vec::new(),
        }
    }

    pub fn add_check(&mut self, check: CheckResult) {
        if check.status == CheckStatus::Error {
            self.ok = false;
        }
        self.checks.push(check);
    }

    pub fn is_ok(&self) -> bool {
        self.ok
    }

    pub fn render_human(&self) -> String {
        let mut lines = Vec::new();
        for check in &self.checks {
            let mut line = format!("{}: {}", check.name, check.status);
            if let Some(details) = &check.message {
                if !details.is_empty() {
                    line.push_str(&format!(" - {}", details));
                }
            }
            lines.push(line);
        }
        lines.join("\n")
    }
}

impl Default for DoctorReport {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub message: Option<String>,
}

impl CheckResult {
    pub fn ok(name: impl Into<String>, message: impl Into<Option<String>>) -> Self {
        CheckResult {
            name: name.into(),
            status: CheckStatus::Ok,
            message: message.into(),
        }
    }

    pub fn warning(name: impl Into<String>, message: impl Into<Option<String>>) -> Self {
        CheckResult {
            name: name.into(),
            status: CheckStatus::Warning,
            message: message.into(),
        }
    }

    pub fn error(name: impl Into<String>, message: impl Into<Option<String>>) -> Self {
        CheckResult {
            name: name.into(),
            status: CheckStatus::Error,
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
}

impl fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckStatus::Ok => write!(f, "OK"),
            CheckStatus::Warning => write!(f, "WARNING"),
            CheckStatus::Error => write!(f, "ERROR"),
        }
    }
}
