use crate::{error::StoreError, layout::StateLayout};
use core::Profile;
use serde_json;
use std::fs::{read_dir, File};
use std::path::PathBuf;
use tempfile::NamedTempFile;

#[derive(Clone, Debug)]
pub struct ProfileStore {
    layout: StateLayout,
}

impl ProfileStore {
    pub fn new(layout: StateLayout) -> Self {
        Self { layout }
    }

    pub fn save_profile(&self, profile: &Profile) -> Result<PathBuf, StoreError> {
        let name = canonical_name(&profile.name)?;
        let path = self.profile_path(&name);
        let temp_file = NamedTempFile::new_in(self.layout.profiles_dir())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = temp_file.as_file().metadata()?;
            let mut perms = metadata.permissions();
            perms.set_mode(0o600);
            temp_file.as_file().set_permissions(perms)?;
        }
        serde_json::to_writer_pretty(temp_file.as_file(), profile)?;
        temp_file.as_file().sync_all()?;
        temp_file
            .persist(&path)
            .map_err(|err| StoreError::Io(err.error))?;
        Ok(path)
    }

    pub fn load_profile(&self, name: &str) -> Result<Profile, StoreError> {
        let canonical = canonical_name(name)?;
        let path = self.profile_path(&canonical);
        let file = File::open(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                StoreError::ProfileNotFound(canonical.clone())
            } else {
                StoreError::Io(err)
            }
        })?;
        let profile = serde_json::from_reader(file)?;
        Ok(profile)
    }

    pub fn list_profiles(&self) -> Result<Vec<Profile>, StoreError> {
        let mut profiles = Vec::new();
        for entry in read_dir(self.layout.profiles_dir())? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            if let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) {
                let profile = self.load_profile(name)?;
                profiles.push(profile);
            }
        }
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(profiles)
    }

    fn profile_path(&self, name: &str) -> PathBuf {
        self.layout
            .profiles_dir()
            .join(format!("{}.json", name))
    }
}

fn canonical_name(name: &str) -> Result<String, StoreError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(StoreError::InvalidProfileName(name.to_string()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.bytes().any(|b| b == 0) {
        return Err(StoreError::InvalidProfileName(trimmed.to_string()));
    }
    Ok(trimmed.to_string())
}
