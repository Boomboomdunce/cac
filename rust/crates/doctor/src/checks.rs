use crate::report::CheckResult;
use crate::DoctorConfig;
use anyhow::Context;
use core::{CapabilitySet, PrivacyPolicy, Profile, TargetAdapter};
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

    let adapter = TargetAdapter::new(
        adapter_name.clone(),
        CapabilitySet::new(),
        CapabilitySet::new(),
        PrivacyPolicy::default(),
    );

    CheckResult::ok(
        "adapter resolution",
        Some(format!("adapter '{}' is supported", adapter.name)),
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
