use crate::error::StoreError;
use std::fs::{File, OpenOptions};
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub fn create_secure_file(path: impl AsRef<Path>) -> Result<File, StoreError> {
    let path = path.as_ref();
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        options.mode(0o600);
    }

    let file = options.open(path)?;
    ensure_secure_permissions(&file)?;

    Ok(file)
}

fn ensure_secure_permissions(file: &File) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        let mut perms = file.metadata()?.permissions();
        perms.set_mode(0o600);
        file.set_permissions(perms)?;
    }
    #[cfg(not(unix))]
    {
        let _ = file.metadata()?;
    }

    Ok(())
}
