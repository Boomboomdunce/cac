use ccp_store::{
    load_profile_identity, ProfileStore, RuntimeStateStore, StateLayout,
};
use serde::Serialize;
use std::path::PathBuf;

fn state_root() -> PathBuf {
    std::env::var("CCP_STATE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".ccp-rust")
        })
}

fn layout() -> Result<StateLayout, String> {
    StateLayout::new(state_root()).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct AppStatus {
    pub active: bool,
    pub paused: bool,
    pub profile: Option<String>,
    pub version: String,
}

#[tauri::command]
pub fn get_status() -> Result<AppStatus, String> {
    let layout = layout()?;
    let runtime = RuntimeStateStore::new(layout);
    let profile = runtime.active_profile().map_err(|e| e.to_string())?;
    let paused = runtime.is_paused();
    Ok(AppStatus {
        active: profile.is_some() && !paused,
        paused,
        profile,
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[derive(Serialize)]
pub struct ProfileSummary {
    pub name: String,
    pub adapter: String,
    pub active: bool,
}

#[tauri::command]
pub fn list_profiles() -> Result<Vec<ProfileSummary>, String> {
    let layout = layout()?;
    let store = ProfileStore::new(layout.clone());
    let runtime = RuntimeStateStore::new(layout);
    let active = runtime.active_profile().unwrap_or(None);

    let profiles = store.list_profiles().map_err(|e| e.to_string())?;
    Ok(profiles
        .into_iter()
        .map(|p| ProfileSummary {
            active: active.as_deref() == Some(&p.name),
            name: p.name,
            adapter: p.adapter,
        })
        .collect())
}

#[derive(Serialize)]
pub struct ProfileIdentityInfo {
    pub uuid: String,
    pub stable_id: String,
    pub user_id: String,
    pub machine_id: String,
    pub hostname: String,
    pub mac_address: String,
    pub tz: String,
    pub lang: String,
}

#[tauri::command]
pub fn get_profile_identity(name: String) -> Result<ProfileIdentityInfo, String> {
    let layout = layout()?;
    let identity = load_profile_identity(&layout, &name).map_err(|e| e.to_string())?;
    Ok(ProfileIdentityInfo {
        uuid: identity.uuid,
        stable_id: identity.stable_id,
        user_id: identity.user_id,
        machine_id: identity.machine_id,
        hostname: identity.hostname,
        mac_address: identity.mac_address,
        tz: identity.tz,
        lang: identity.lang,
    })
}

#[tauri::command]
pub fn switch_profile(name: String) -> Result<(), String> {
    let layout = layout()?;
    let store = ProfileStore::new(layout.clone());
    // Verify profile exists
    store.load_profile(&name).map_err(|e| e.to_string())?;
    let runtime = RuntimeStateStore::new(layout);
    runtime.set_active_profile(&name).map_err(|e| e.to_string())?;
    runtime.set_paused(false).map_err(|e| e.to_string())?;
    Ok(())
}
