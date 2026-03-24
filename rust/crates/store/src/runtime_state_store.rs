use crate::{error::StoreError, layout::StateLayout, secret_store::create_secure_file};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct RuntimeStateStore {
    layout: StateLayout,
}

impl RuntimeStateStore {
    pub fn new(layout: StateLayout) -> Self {
        Self { layout }
    }

    pub fn active_profile(&self) -> Result<Option<String>, StoreError> {
        let path = self.current_profile_path();
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let trimmed = contents.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed.to_string()))
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(StoreError::Io(err)),
        }
    }

    pub fn set_active_profile(&self, profile_name: &str) -> Result<(), StoreError> {
        let mut file = create_secure_file(self.current_profile_path())?;
        file.write_all(profile_name.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    }

    pub fn clear_active_profile(&self) -> Result<(), StoreError> {
        remove_if_exists(&self.current_profile_path())
    }

    pub fn is_paused(&self) -> bool {
        self.paused_path().is_file()
    }

    pub fn set_paused(&self, paused: bool) -> Result<(), StoreError> {
        if paused {
            let mut file = create_secure_file(self.paused_path())?;
            file.write_all(b"paused\n")?;
            file.sync_all()?;
            Ok(())
        } else {
            remove_if_exists(&self.paused_path())
        }
    }

    fn current_profile_path(&self) -> PathBuf {
        self.layout.config_dir().join("current_profile")
    }

    fn paused_path(&self) -> PathBuf {
        self.layout.config_dir().join("paused")
    }
}

fn remove_if_exists(path: &PathBuf) -> Result<(), StoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(StoreError::Io(err)),
    }
}
