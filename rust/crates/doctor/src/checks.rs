use crate::report::CheckResult;
use crate::DoctorConfig;
use anyhow::Context;
use claude_adapter::{claude_adapter, ADAPTER_NAME};
use core::{CapabilitySet, PlatformDoctorCheck, PrivacyPolicy, Profile, TargetAdapter};
use serde_json::from_reader;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

const STATE_DIRS: &[&str] = &[
    "profiles",
    "identities",
    "certs",
    "hooks",
    "sessions",
    "sidecar",
    "audit",
    "config",
];

const SECRET_DIRS: &[&str] = &["profiles", "identities", "certs", "hooks"];

const SUPPORTED_ADAPTERS: &[&str] = &["claude"];

pub fn profile_existence(config: &DoctorConfig) -> CheckResult {
    match canonical_profile_name(config.profile()) {
        Ok(name) => {
            let path = profile_file_path(config.state_root(), &name);
            if path.is_file() {
                CheckResult::ok(
                    "profile existence",
                    Some(format!("profile '{}' exists at {}", name, path.display())),
                )
            } else {
                CheckResult::error(
                    "profile existence",
                    Some(format!(
                        "profile '{}' not found (expected {})",
                        name,
                        path.display()
                    )),
                )
            }
        }
        Err(err) => CheckResult::error("profile existence", Some(err)),
    }
}

pub fn state_root_layout(config: &DoctorConfig) -> CheckResult {
    let missing: Vec<String> = STATE_DIRS
        .iter()
        .filter(|dir| !config.state_root().join(dir).is_dir())
        .map(|dir| dir.to_string())
        .collect();

    if missing.is_empty() {
        CheckResult::ok(
            "State root layout",
            Some("all expected directories are present".to_string()),
        )
    } else {
        CheckResult::error(
            "State root layout",
            Some(format!("missing directories: {}", missing.join(", "))),
        )
    }
}

pub fn adapter_resolution(config: &DoctorConfig) -> CheckResult {
    let name = match canonical_profile_name(config.profile()) {
        Ok(name) => name,
        Err(err) => return CheckResult::error("adapter resolution", Some(err)),
    };

    let profile = match load_profile(config.state_root(), &name) {
        Ok(profile) => profile,
        Err(err) => {
            return CheckResult::error(
                "adapter resolution",
                Some(format!("failed to load profile: {}", err)),
            )
        }
    };

    let adapter_name = profile.adapter.clone();
    if !SUPPORTED_ADAPTERS.contains(&adapter_name.as_str()) {
        return CheckResult::error(
            "adapter resolution",
            Some(format!("unsupported adapter '{}'", adapter_name)),
        );
    }

    let required_capabilities = required_capabilities_for_adapter(adapter_name.as_str());
    let platform_identity = current_platform_identity();
    let provided_capabilities = current_platform_capabilities();
    let missing_capabilities = required_capabilities.difference(&provided_capabilities);
    if !missing_capabilities.is_empty() {
        return CheckResult::error(
            "adapter resolution",
            Some(format!(
                "adapter '{}' is unsupported on '{}' due to missing required capabilities: {}",
                adapter_name,
                platform_identity,
                render_capability_set(&missing_capabilities)
            )),
        );
    }

    let adapter = TargetAdapter::new(
        adapter_name.clone(),
        required_capabilities,
        CapabilitySet::new(),
        PrivacyPolicy::default(),
    );

    CheckResult::ok(
        "adapter resolution",
        Some(format!(
            "adapter '{}' is supported on '{}' with capabilities: {}",
            adapter.name,
            platform_identity,
            render_capability_set(&provided_capabilities)
        )),
    )
}

pub fn secret_permission_sanity(config: &DoctorConfig) -> CheckResult {
    let mut violations = Vec::new();
    let mut missing = Vec::new();

    for dir in SECRET_DIRS {
        let path = config.state_root().join(dir);
        if !path.exists() {
            missing.push(dir.to_string());
            continue;
        }

        match has_owner_only_permissions(&path) {
            Ok(true) => {}
            Ok(false) => violations.push(dir.to_string()),
            Err(err) => {
                return CheckResult::error(
                    "secret permission sanity",
                    Some(format!("cannot stat {}: {}", path.display(), err)),
                );
            }
        }
    }

    if !violations.is_empty() {
        return CheckResult::error(
            "secret permission sanity",
            Some(format!(
                "directories with weak permissions: {}",
                violations.join(", ")
            )),
        );
    }

    if !missing.is_empty() {
        return CheckResult::warning(
            "secret permission sanity",
            Some(format!(
                "missing secret directories: {}",
                missing.join(", ")
            )),
        );
    }

    CheckResult::ok(
        "secret permission sanity",
        Some("directory permissions look appropriate".to_string()),
    )
}

pub fn platform_capability_support(config: &DoctorConfig) -> CheckResult {
    let name = match canonical_profile_name(config.profile()) {
        Ok(name) => name,
        Err(err) => return CheckResult::error("platform capability support", Some(err)),
    };

    let profile = match load_profile(config.state_root(), &name) {
        Ok(profile) => profile,
        Err(err) => {
            return CheckResult::error(
                "platform capability support",
                Some(format!("failed to load profile: {}", err)),
            )
        }
    };

    let adapter = match resolve_adapter(&profile) {
        Ok(adapter) => adapter,
        Err(err) => return CheckResult::error("platform capability support", Some(err)),
    };

    let platform_checks = current_platform_doctor_checks(&adapter.required_capabilities);
    summarize_platform_doctor_checks(platform_checks)
}

fn summarize_platform_doctor_checks(checks: Vec<PlatformDoctorCheck>) -> CheckResult {
    if checks.is_empty() {
        return CheckResult::error(
            "platform capability support",
            Some(format!(
                "platform '{}' did not return any doctor checks",
                current_platform_identity()
            )),
        );
    }

    let has_error = checks.iter().any(|check| !check.ok);
    let message = checks
        .iter()
        .map(|check| check.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");

    if has_error {
        CheckResult::error("platform capability support", Some(message))
    } else {
        CheckResult::ok("platform capability support", Some(message))
    }
}

fn canonical_profile_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("profile name is empty".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.as_bytes().contains(&0) {
        return Err("profile name contains invalid characters".to_string());
    }
    Ok(trimmed.to_string())
}

fn profile_file_path(root: &Path, name: &str) -> PathBuf {
    root.join("profiles").join(format!("{}.json", name))
}

fn load_profile(root: &Path, name: &str) -> anyhow::Result<Profile> {
    let path = profile_file_path(root, name);
    let file = File::open(&path).context("opening profile metadata")?;
    let profile = from_reader(file).context("parsing profile metadata")?;
    Ok(profile)
}

fn resolve_adapter(profile: &Profile) -> Result<TargetAdapter, String> {
    if profile.adapter.eq_ignore_ascii_case(ADAPTER_NAME) {
        Ok(claude_adapter().target_adapter().clone())
    } else {
        Err(format!("unsupported adapter '{}'", profile.adapter))
    }
}

#[cfg(target_os = "macos")]
fn current_platform_identity() -> &'static str {
    platform_macos::platform_identity()
}

#[cfg(target_os = "linux")]
fn current_platform_identity() -> &'static str {
    platform_linux::platform_identity()
}

#[cfg(target_os = "windows")]
fn current_platform_identity() -> &'static str {
    platform_windows::platform_identity()
}

#[cfg(target_os = "macos")]
fn current_platform_doctor_checks(required: &CapabilitySet) -> Vec<PlatformDoctorCheck> {
    platform_macos::doctor_checks(required)
}

#[cfg(target_os = "macos")]
fn current_platform_capabilities() -> CapabilitySet {
    platform_macos::provided_capabilities()
}

#[cfg(target_os = "linux")]
fn current_platform_doctor_checks(required: &CapabilitySet) -> Vec<PlatformDoctorCheck> {
    platform_linux::doctor_checks(required)
}

#[cfg(target_os = "linux")]
fn current_platform_capabilities() -> CapabilitySet {
    platform_linux::provided_capabilities()
}

#[cfg(target_os = "windows")]
fn current_platform_doctor_checks(required: &CapabilitySet) -> Vec<PlatformDoctorCheck> {
    platform_windows::doctor_checks(required)
}

#[cfg(target_os = "windows")]
fn current_platform_capabilities() -> CapabilitySet {
    platform_windows::provided_capabilities()
}

fn has_owner_only_permissions(path: &Path) -> std::io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let metadata = path.metadata()?;
        let mode = metadata.permissions().mode();
        Ok(mode & 0o077 == 0)
    }

    #[cfg(not(unix))]
    {
        let _ = path.metadata()?;
        Ok(true)
    }
}

fn required_capabilities_for_adapter(adapter_name: &str) -> CapabilitySet {
    if adapter_name.eq_ignore_ascii_case("claude") {
        CapabilitySet::from(["node_preload", "sidecar"])
    } else {
        CapabilitySet::new()
    }
}

fn render_capability_set(capabilities: &CapabilitySet) -> String {
    if capabilities.is_empty() {
        return "<none>".to_string();
    }

    capabilities
        .iter()
        .map(|value| value.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_platform_doctor_checks_keeps_all_messages_and_error_status() {
        let result = summarize_platform_doctor_checks(vec![
            PlatformDoctorCheck::ok(
                "platform capability support",
                "node preload support is available",
            ),
            PlatformDoctorCheck::error("platform capability support", "sidecar bridge is missing"),
        ]);

        assert_eq!(result.name, "platform capability support");
        assert_eq!(result.status, crate::report::CheckStatus::Error);
        let message = result.message.expect("expected aggregated message");
        assert!(message.contains("node preload support is available"));
        assert!(message.contains("sidecar bridge is missing"));
    }
}
