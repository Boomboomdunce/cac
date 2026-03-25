use crate::{error::StoreError, layout::StateLayout, secret_store::create_secure_file};
use core::{ClaudeProviderConfig, Profile};
use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};

#[derive(Clone, Debug)]
pub struct ManagedClaudeConfig {
    pub root: PathBuf,
}

pub fn materialize_managed_claude_config(
    layout: &StateLayout,
    profile: &Profile,
) -> Result<ManagedClaudeConfig, StoreError> {
    let root = layout
        .config_dir()
        .join("claude-config")
        .join(&profile.name);
    fs::create_dir_all(&root)?;
    set_owner_only(&root)?;

    let user_claude_dir = user_claude_dir();
    let user_home = home_dir();

    materialize_settings_json(
        user_claude_dir
            .as_deref()
            .map(|dir| dir.join("settings.json")),
        root.join("settings.json"),
        profile
            .claude
            .as_ref()
            .and_then(|claude| claude.provider.as_ref()),
    )?;
    materialize_claude_json(
        user_home.as_deref().map(|dir| dir.join(".claude.json")),
        root.join(".claude.json"),
    )?;

    for relative in ["skills", "commands", "plugins", "agents"] {
        let source = user_claude_dir.as_deref().map(|dir| dir.join(relative));
        let destination = root.join(relative);
        materialize_optional_dir(source.as_deref(), &destination)?;
    }

    fs::create_dir_all(root.join("statsig"))?;
    set_owner_only(&root.join("statsig"))?;

    Ok(ManagedClaudeConfig { root })
}

fn materialize_settings_json(
    source: Option<PathBuf>,
    destination: PathBuf,
    provider: Option<&ClaudeProviderConfig>,
) -> Result<(), StoreError> {
    let mut document = if destination.is_file() {
        read_json_or_default_object(&destination)?
    } else {
        match source {
            Some(path) if path.is_file() => read_json_or_default_object(&path)?,
            _ => Value::Object(Map::new()),
        }
    };
    apply_provider_settings(&mut document, provider);
    write_json_file(&destination, &document)
}

fn materialize_claude_json(
    source: Option<PathBuf>,
    destination: PathBuf,
) -> Result<(), StoreError> {
    let document = match source {
        Some(path) if path.is_file() => read_json_or_default_object(&path)?,
        _ => Value::Object(Map::new()),
    };
    let object = match document {
        Value::Object(map) => Value::Object(map),
        _ => Value::Object(Map::new()),
    };
    write_json_file(&destination, &object)
}

fn write_json_file(path: &Path, value: &Value) -> Result<(), StoreError> {
    let mut file = create_secure_file(path)?;
    let rendered = serde_json::to_string_pretty(value)?;
    file.write_all(rendered.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

pub fn snapshot_user_claude_provider() -> Result<Option<ClaudeProviderConfig>, StoreError> {
    let Some(path) = user_claude_dir().map(|dir| dir.join("settings.json")) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }

    let document = read_json_or_default_object(&path)?;
    let Some(env_map) = document.get("env").and_then(Value::as_object) else {
        return Ok(None);
    };

    let provider = ClaudeProviderConfig {
        base_url: env_map
            .get("ANTHROPIC_BASE_URL")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        auth_token: env_map
            .get("ANTHROPIC_AUTH_TOKEN")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        api_key: env_map
            .get("ANTHROPIC_API_KEY")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    };

    if provider.base_url.is_none() && provider.auth_token.is_none() && provider.api_key.is_none() {
        Ok(None)
    } else {
        Ok(Some(provider))
    }
}

fn apply_provider_settings(document: &mut Value, provider: Option<&ClaudeProviderConfig>) {
    let Some(provider) = provider else {
        return;
    };

    let Some(root) = document.as_object_mut() else {
        *document = Value::Object(Map::new());
        apply_provider_settings(document, Some(provider));
        return;
    };

    let env_value = root
        .entry("env".to_string())
        .or_insert_with(|| Value::Object(Map::new()));

    let env_map = match env_value {
        Value::Object(map) => map,
        _ => {
            *env_value = Value::Object(Map::new());
            env_value.as_object_mut().expect("env object just inserted")
        }
    };

    env_map.remove("ANTHROPIC_BASE_URL");
    env_map.remove("ANTHROPIC_AUTH_TOKEN");
    env_map.remove("ANTHROPIC_API_KEY");

    if let Some(base_url) = &provider.base_url {
        env_map.insert(
            "ANTHROPIC_BASE_URL".to_string(),
            Value::String(base_url.clone()),
        );
    }
    if let Some(auth_token) = &provider.auth_token {
        env_map.insert(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            Value::String(auth_token.clone()),
        );
    }
    if let Some(api_key) = &provider.api_key {
        env_map.insert(
            "ANTHROPIC_API_KEY".to_string(),
            Value::String(api_key.clone()),
        );
    }

    if env_map.is_empty() {
        root.remove("env");
    }
}

fn read_json_or_default_object(path: &Path) -> Result<Value, StoreError> {
    let contents = fs::read_to_string(path)?;
    if contents.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }

    match serde_json::from_str::<Value>(&contents) {
        Ok(value) => Ok(value),
        Err(err) if err.is_eof() => Ok(Value::Object(Map::new())),
        Err(err) => Err(err.into()),
    }
}

fn materialize_optional_dir(source: Option<&Path>, destination: &Path) -> Result<(), StoreError> {
    remove_path_if_exists(destination)?;

    let Some(source) = source else {
        return Ok(());
    };
    if !source.exists() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        symlink(source, destination)?;
        Ok(())
    }

    #[cfg(not(unix))]
    {
        copy_dir_recursive(source, destination)?;
        Ok(())
    }
}

fn remove_path_if_exists(path: &Path) -> Result<(), StoreError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };

    let file_type = metadata.file_type();
    if file_type.is_symlink() || file_type.is_file() {
        fs::remove_file(path)?;
    } else if file_type.is_dir() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), StoreError> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn user_claude_dir() -> Option<PathBuf> {
    home_dir().map(|path| path.join(".claude"))
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
}

#[cfg(unix)]
fn set_owner_only(path: &Path) -> Result<(), StoreError> {
    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn materialize_managed_config_tolerates_empty_user_settings_file() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let state = temp.path().join("state");
        let user_claude_dir = home.join(".claude");
        std::fs::create_dir_all(&user_claude_dir).unwrap();
        std::fs::write(user_claude_dir.join("settings.json"), "").unwrap();
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home);

        let layout = StateLayout::new(&state).unwrap();
        let profile = Profile::new("work", "claude", core::PrivacyPolicy::default());
        let managed = materialize_managed_claude_config(&layout, &profile).unwrap();

        let settings = std::fs::read_to_string(managed.root.join("settings.json")).unwrap();
        assert!(settings.contains("{"));

        match previous_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
    }
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}
