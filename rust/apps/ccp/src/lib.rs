pub mod audit;
pub mod install;

use std::{
    env, fs, io,
    path::{Path, PathBuf},
};
use store::{ProfileStore, RuntimeStateStore, StateLayout};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupStatus {
    pub state_root: PathBuf,
    pub wrappers_installed: bool,
    pub install_metadata_present: bool,
    pub ccp_binary_path: Option<PathBuf>,
    pub suggested_bin_dir: PathBuf,
    pub suggested_shell_rc: Option<PathBuf>,
    pub profiles: Vec<String>,
    pub active_profile: Option<String>,
    pub active_profile_has_proxy: bool,
}

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
        .or_else(|| {
            let drive = env::var_os("HOMEDRIVE")?;
            let path = env::var_os("HOMEPATH")?;
            let mut combined = PathBuf::from(drive);
            combined.push(path);
            Some(combined)
        })
}

pub fn default_state_root() -> Result<PathBuf, io::Error> {
    if let Some(root) = env::var_os("CCP_STATE_ROOT") {
        Ok(PathBuf::from(root))
    } else {
        home_dir()
            .map(|home| home.join(".ccp-rust"))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "could not determine home directory for default CCP state root",
                )
            })
    }
}

pub fn inspect_setup_status(state_root: &Path) -> SetupStatus {
    let layout = StateLayout::new(state_root.to_path_buf()).ok();
    let (profiles, active_profile, active_profile_has_proxy) = if let Some(layout) = &layout {
        let store = ProfileStore::new(layout.clone());
        let runtime = RuntimeStateStore::new(layout.clone());
        let profiles = store
            .list_profiles()
            .map(|items| items.into_iter().map(|p| p.name).collect())
            .unwrap_or_default();
        let active_profile = runtime.active_profile().unwrap_or(None);
        let active_profile_has_proxy = active_profile
            .as_deref()
            .and_then(|name| store.load_profile(name).ok())
            .and_then(|profile| profile.policy.proxy_url().map(|_| true))
            .unwrap_or(false);
        (profiles, active_profile, active_profile_has_proxy)
    } else {
        (Vec::new(), None, false)
    };

    let install_metadata = install::load_install_metadata(state_root).ok();
    let wrappers_installed = install_metadata
        .as_ref()
        .map(|metadata| metadata.generated_paths.iter().all(|path| path.is_file()))
        .unwrap_or(false);

    let suggested_bin_dir = home_dir()
        .map(|home| home.join("bin"))
        .unwrap_or_else(|| PathBuf::from("bin"));
    let suggested_shell_rc = install::detect_shell_rc();

    SetupStatus {
        state_root: state_root.to_path_buf(),
        wrappers_installed,
        install_metadata_present: install_metadata.is_some(),
        ccp_binary_path: discover_ccp_binary(install_metadata.as_ref()),
        suggested_bin_dir,
        suggested_shell_rc,
        profiles,
        active_profile,
        active_profile_has_proxy,
    }
}

pub fn discover_ccp_binary(install_metadata: Option<&install::InstallMetadata>) -> Option<PathBuf> {
    if let Some(path) = env::var_os("CCP_BIN_PATH").map(PathBuf::from) {
        if is_executable_file(&path) {
            return Some(path);
        }
    }

    if let Some(path) = env::var_os("CARGO_BIN_EXE_ccp").map(PathBuf::from) {
        if is_executable_file(&path) {
            return Some(path);
        }
    }

    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let sibling = dir.join(install::wrapper_name("ccp"));
            if sibling != current_exe && is_executable_file(&sibling) {
                return Some(sibling);
            }
        }
    }

    if let Some(metadata) = install_metadata {
        if is_executable_file(&metadata.ccp_bin_path) {
            return Some(metadata.ccp_bin_path.clone());
        }
    }

    find_executable_in_path(install::wrapper_name("ccp").as_str())
}

fn find_executable_in_path(program: &str) -> Option<PathBuf> {
    let path_entries = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();

    let home_bin = home_dir().map(|home| home.join("bin"));

    path_entries.into_iter().find_map(|entry| {
        let candidate = entry.join(program);
        if !is_executable_file(&candidate) {
            return None;
        }

        if home_bin
            .as_ref()
            .is_some_and(|bin_dir| candidate.starts_with(bin_dir))
        {
            return None;
        }

        Some(candidate)
    })
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn inspect_setup_status_reports_missing_profile_and_wrapper_state() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("config")).unwrap();

        let status = inspect_setup_status(temp.path());

        assert!(status.profiles.is_empty());
        assert!(status.active_profile.is_none());
        assert!(!status.active_profile_has_proxy);
        assert!(!status.wrappers_installed);
    }
}
