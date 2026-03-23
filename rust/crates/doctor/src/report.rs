use core::redact_sensitive_text;
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
            message: sanitize_message(message.into()),
        }
    }

    pub fn warning(name: impl Into<String>, message: impl Into<Option<String>>) -> Self {
        CheckResult {
            name: name.into(),
            status: CheckStatus::Warning,
            message: sanitize_message(message.into()),
        }
    }

    pub fn error(name: impl Into<String>, message: impl Into<Option<String>>) -> Self {
        CheckResult {
            name: name.into(),
            status: CheckStatus::Error,
            message: sanitize_message(message.into()),
        }
    }
}

fn sanitize_message(message: Option<String>) -> Option<String> {
    message.map(|value| redact_sensitive_text(&value))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_serialization_redacts_proxy_credentials_in_messages() {
        let mut report = DoctorReport::new();
        report.add_check(CheckResult::ok(
            "proxy check",
            Some("using proxy https://alice:super-secret@proxy.example:8443".to_string()),
        ));

        let rendered = serde_json::to_string(&report).expect("report should serialize");
        assert!(rendered.contains("https://alice:***@proxy.example:8443"));
        assert!(!rendered.contains("super-secret"));
    }
}
