use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};
use store::StateLayout;

const SHELL_BLOCK_START: &str = "# >>> ccp >>>";
const SHELL_BLOCK_END: &str = "# <<< ccp <<<";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallMetadata {
    pub bin_dir: PathBuf,
    pub shell_rc: Option<PathBuf>,
    pub real_claude_path: PathBuf,
    pub ccp_bin_path: PathBuf,
    pub generated_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct SetupConfig {
    pub bin_dir: PathBuf,
    pub shell_rc: Option<PathBuf>,
    pub ccp_bin_path: PathBuf,
}

pub fn setup(layout: &StateLayout, config: SetupConfig) -> Result<InstallMetadata> {
    let real_claude_path =
        resolve_real_claude(config.bin_dir.as_path()).context("finding real claude executable")?;
    fs::create_dir_all(&config.bin_dir)
        .with_context(|| format!("creating {}", config.bin_dir.display()))?;

    let ccp_wrapper = config.bin_dir.join(wrapper_name("ccp"));
    let claude_wrapper = config.bin_dir.join(wrapper_name("claude"));

    write_wrapper(
        &ccp_wrapper,
        ccp_shim_contents(config.ccp_bin_path.as_path()).as_str(),
    )?;
    write_wrapper(
        &claude_wrapper,
        claude_wrapper_contents(
            config.ccp_bin_path.as_path(),
            layout.root(),
            real_claude_path.as_path(),
        )
        .as_str(),
    )?;

    if let Some(shell_rc) = &config.shell_rc {
        write_shell_block(shell_rc, config.bin_dir.as_path())?;
    }

    fs::write(
        layout.config_dir().join("real_claude_path"),
        format!("{}\n", real_claude_path.display()),
    )
    .with_context(|| {
        format!(
            "writing {}",
            layout.config_dir().join("real_claude_path").display()
        )
    })?;

    let metadata = InstallMetadata {
        bin_dir: config.bin_dir,
        shell_rc: config.shell_rc,
        real_claude_path,
        ccp_bin_path: config.ccp_bin_path,
        generated_paths: vec![ccp_wrapper, claude_wrapper],
    };
    fs::write(
        layout.config_dir().join("install.json"),
        format!("{}\n", serde_json::to_string_pretty(&metadata)?),
    )
    .with_context(|| {
        format!(
            "writing {}",
            layout.config_dir().join("install.json").display()
        )
    })?;

    Ok(metadata)
}

pub fn uninstall(state_root: &Path) -> Result<()> {
    let metadata = load_install_metadata(state_root)?;

    for path in &metadata.generated_paths {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err).with_context(|| format!("removing {}", path.display())),
        }
    }

    if let Some(shell_rc) = &metadata.shell_rc {
        remove_shell_block(shell_rc)?;
    }

    match fs::remove_dir_all(state_root) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("removing {}", state_root.display())),
    }
}

pub fn detect_shell_rc() -> Option<PathBuf> {
    let home_dir = env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))?;

    [".zshrc", ".bashrc", ".bash_profile"]
        .into_iter()
        .map(|name| home_dir.join(name))
        .find(|path| path.is_file())
        .or_else(|| Some(home_dir.join(".zshrc")))
}

pub fn load_install_metadata(state_root: &Path) -> Result<InstallMetadata> {
    let install_path = state_root.join("config/install.json");
    let contents = fs::read_to_string(&install_path)
        .with_context(|| format!("reading {}", install_path.display()))?;
    let metadata = serde_json::from_str(&contents)
        .with_context(|| format!("parsing {}", install_path.display()))?;
    Ok(metadata)
}

fn resolve_real_claude(bin_dir: &Path) -> Result<PathBuf> {
    let current_real = detect_real_claude_from_path(bin_dir)?;
    Ok(current_real)
}

fn detect_real_claude_from_path(bin_dir: &Path) -> Result<PathBuf> {
    let path_entries = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();

    for entry in path_entries {
        if entry == bin_dir {
            continue;
        }
        let candidate = entry.join(wrapper_name("claude"));
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(anyhow::anyhow!("could not locate `claude` in PATH"))
}

fn write_shell_block(shell_rc: &Path, bin_dir: &Path) -> Result<()> {
    let existing = fs::read_to_string(shell_rc).unwrap_or_default();
    if existing.contains(SHELL_BLOCK_START) {
        return Ok(());
    }

    if let Some(parent) = shell_rc.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(shell_rc)
        .with_context(|| format!("opening {}", shell_rc.display()))?;
    writeln!(file)?;
    writeln!(file, "{SHELL_BLOCK_START}")?;
    writeln!(file, "export PATH=\"{}:$PATH\"", bin_dir.display())?;
    writeln!(file, "{SHELL_BLOCK_END}")?;
    file.sync_all()
        .with_context(|| format!("syncing {}", shell_rc.display()))?;
    Ok(())
}

fn remove_shell_block(shell_rc: &Path) -> Result<()> {
    let existing = match fs::read_to_string(shell_rc) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(|| format!("reading {}", shell_rc.display())),
    };

    let mut filtered = Vec::new();
    let mut skipping = false;
    for line in existing.lines() {
        if line.trim() == SHELL_BLOCK_START {
            skipping = true;
            continue;
        }
        if line.trim() == SHELL_BLOCK_END {
            skipping = false;
            continue;
        }
        if !skipping {
            filtered.push(line);
        }
    }

    let mut rendered = filtered.join("\n");
    if !rendered.is_empty() {
        rendered.push('\n');
    }
    fs::write(shell_rc, rendered).with_context(|| format!("writing {}", shell_rc.display()))?;
    Ok(())
}

fn write_wrapper(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn ccp_shim_contents(ccp_bin_path: &Path) -> String {
    #[cfg(unix)]
    {
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nexec \"{}\" \"$@\"\n",
            ccp_bin_path.display()
        )
    }
    #[cfg(windows)]
    {
        format!("@echo off\r\n\"{}\" %*\r\n", ccp_bin_path.display())
    }
}

fn claude_wrapper_contents(ccp_bin_path: &Path, state_root: &Path, real_claude: &Path) -> String {
    #[cfg(unix)]
    {
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nexport CCP_STATE_ROOT=\"{}\"\nexec \"{}\" run -- \"{}\" \"$@\"\n",
            state_root.display(),
            ccp_bin_path.display(),
            real_claude.display()
        )
    }
    #[cfg(windows)]
    {
        format!(
            "@echo off\r\nset CCP_STATE_ROOT={}\r\n\"{}\" run -- \"{}\" %*\r\n",
            state_root.display(),
            ccp_bin_path.display(),
            real_claude.display()
        )
    }
}

pub fn wrapper_name(base: &str) -> String {
    #[cfg(windows)]
    {
        format!("{base}.cmd")
    }
    #[cfg(not(windows))]
    {
        base.to_string()
    }
}
