use crate::error::StoreError;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Clone, Debug)]
pub struct StateLayout {
    root: PathBuf,
    profiles: PathBuf,
    identities: PathBuf,
    certs: PathBuf,
    hooks: PathBuf,
    sessions: PathBuf,
    sidecar: PathBuf,
    audit: PathBuf,
    config: PathBuf,
}

impl StateLayout {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        set_owner_only(&root)?;

        let layout = StateLayout {
            root: root.clone(),
            profiles: root.join("profiles"),
            identities: root.join("identities"),
            certs: root.join("certs"),
            hooks: root.join("hooks"),
            sessions: root.join("sessions"),
            sidecar: root.join("sidecar"),
            audit: root.join("audit"),
            config: root.join("config"),
        };

        layout.create_dirs()?;
        Ok(layout)
    }

    fn create_dirs(&self) -> Result<(), StoreError> {
        for dir in [
            &self.profiles,
            &self.identities,
            &self.certs,
            &self.hooks,
            &self.sessions,
            &self.sidecar,
            &self.audit,
            &self.config,
        ] {
            fs::create_dir_all(dir)?;
            set_owner_only(dir)?;
        }
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn profiles_dir(&self) -> &Path {
        &self.profiles
    }

    pub fn identities_dir(&self) -> &Path {
        &self.identities
    }

    pub fn certs_dir(&self) -> &Path {
        &self.certs
    }

    pub fn hooks_dir(&self) -> &Path {
        &self.hooks
    }

    pub fn sessions_dir(&self) -> &Path {
        &self.sessions
    }

    pub fn sidecar_dir(&self) -> &Path {
        &self.sidecar
    }

    pub fn audit_dir(&self) -> &Path {
        &self.audit
    }

    pub fn config_dir(&self) -> &Path {
        &self.config
    }
}

#[cfg(unix)]
fn set_owner_only(path: &Path) -> Result<(), StoreError> {
    let metadata = fs::metadata(path)?;
    let mut perms = metadata.permissions();
    perms.set_mode(0o700);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}
