use ccp_store::{
    ensure_profile_certificates, ensure_profile_identity_seeded, load_profile_identity,
    certificate_material, ProfileStore, RuntimeStateStore, StateLayout,
};
use ccp_core::{PrivacyPolicy, Profile};
use serde::{Deserialize, Serialize};
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

// ── Status ──

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

// ── Profiles ──

#[derive(Serialize)]
pub struct ProfileSummary {
    pub name: String,
    pub adapter: String,
    pub proxy_url: Option<String>,
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
        .map(|p| {
            let proxy_url = p.policy.proxy_url().map(|u| {
                ccp_core::redact_proxy_url(u)
            });
            ProfileSummary {
                active: active.as_deref() == Some(&p.name),
                name: p.name,
                adapter: p.adapter,
                proxy_url,
            }
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

#[derive(Deserialize)]
pub struct CreateProfileInput {
    pub name: String,
    pub adapter: String,
    pub proxy_url: Option<String>,
    pub timezone: Option<String>,
    pub language: Option<String>,
}

#[tauri::command]
pub fn create_profile(input: CreateProfileInput) -> Result<ProfileIdentityInfo, String> {
    let layout = layout()?;
    let store = ProfileStore::new(layout.clone());

    let mut policy = PrivacyPolicy::new();
    if let Some(ref proxy) = input.proxy_url {
        policy = policy.with_proxy_url(proxy.clone());
    }

    let profile = Profile::new(&input.name, &input.adapter, policy);
    store.save_profile(&profile).map_err(|e| e.to_string())?;

    // Generate identity materials
    ensure_profile_identity_seeded(
        &layout,
        &input.name,
        input.timezone.as_deref(),
        input.language.as_deref(),
    )
    .map_err(|e| e.to_string())?;

    // Generate certificates
    let _ = ensure_profile_certificates(&layout, &input.name);

    // Return the generated identity
    get_profile_identity(input.name)
}

#[tauri::command]
pub fn delete_profile(name: String) -> Result<(), String> {
    let layout = layout()?;
    let store = ProfileStore::new(layout.clone());
    let runtime = RuntimeStateStore::new(layout);

    // If this is the active profile, clear it
    if let Ok(Some(active)) = runtime.active_profile() {
        if active == name {
            runtime.clear_active_profile().map_err(|e| e.to_string())?;
        }
    }

    store.delete_profile(&name).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn switch_profile(name: String) -> Result<(), String> {
    let layout = layout()?;
    let store = ProfileStore::new(layout.clone());
    store.load_profile(&name).map_err(|e| e.to_string())?;
    let runtime = RuntimeStateStore::new(layout);
    runtime.set_active_profile(&name).map_err(|e| e.to_string())?;
    runtime.set_paused(false).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn toggle_protection(enable: bool) -> Result<(), String> {
    let layout = layout()?;
    let runtime = RuntimeStateStore::new(layout);
    runtime.set_paused(!enable).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Profile Import/Export ──

#[derive(Serialize, Deserialize)]
pub struct ExportedProfile {
    pub format_version: u32,
    pub export_type: String,
    pub name: String,
    pub proxy_url: Option<String>,
    pub adapter: String,
    pub identity: Option<ExportedIdentity>,
    pub locale: ExportedLocale,
}

#[derive(Serialize, Deserialize)]
pub struct ExportedIdentity {
    pub uuid: String,
    pub stable_id: String,
    pub user_id: String,
    pub machine_id: String,
    pub hostname: String,
    pub mac_address: String,
}

#[derive(Serialize, Deserialize)]
pub struct ExportedLocale {
    pub timezone: String,
    pub language: String,
}

#[tauri::command]
pub fn export_profile(name: String, export_type: String) -> Result<String, String> {
    let layout = layout()?;
    let store = ProfileStore::new(layout.clone());
    let profile = store.load_profile(&name).map_err(|e| e.to_string())?;
    let identity = load_profile_identity(&layout, &name).map_err(|e| e.to_string())?;

    let exported = ExportedProfile {
        format_version: 1,
        export_type: export_type.clone(),
        name: profile.name,
        proxy_url: match export_type.as_str() {
            "redacted" => profile.policy.proxy_url().map(|u| ccp_core::redact_proxy_url(u)),
            _ => profile.policy.proxy_url().map(String::from),
        },
        adapter: profile.adapter,
        identity: match export_type.as_str() {
            "template" => None,
            _ => Some(ExportedIdentity {
                uuid: identity.uuid,
                stable_id: identity.stable_id,
                user_id: identity.user_id,
                machine_id: identity.machine_id,
                hostname: identity.hostname,
                mac_address: identity.mac_address,
            }),
        },
        locale: ExportedLocale {
            timezone: identity.tz,
            language: identity.lang,
        },
    };

    serde_json::to_string_pretty(&exported).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_profile(json_content: String) -> Result<ProfileIdentityInfo, String> {
    let imported: ExportedProfile =
        serde_json::from_str(&json_content).map_err(|e| format!("Invalid profile JSON: {e}"))?;

    let input = CreateProfileInput {
        name: imported.name,
        adapter: imported.adapter,
        proxy_url: imported.proxy_url,
        timezone: Some(imported.locale.timezone),
        language: Some(imported.locale.language),
    };

    create_profile(input)
}

// ── Diagnostics ──

#[derive(Serialize)]
pub struct DiagnosticCheck {
    pub name: String,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Serialize)]
pub struct DiagnosticReport {
    pub ok: bool,
    pub checks: Vec<DiagnosticCheck>,
}

#[tauri::command]
pub fn run_diagnostics(profile_name: String) -> Result<DiagnosticReport, String> {
    let config = ccp_doctor::DoctorConfig::new(state_root(), profile_name);
    let report = ccp_doctor::run(config);

    Ok(DiagnosticReport {
        ok: report.ok,
        checks: report
            .checks
            .into_iter()
            .map(|c| DiagnosticCheck {
                name: c.name,
                status: format!("{}", c.status),
                message: c.message,
            })
            .collect(),
    })
}

// ── Certificate Info ──

#[derive(Serialize)]
pub struct CertInfo {
    pub exists: bool,
    pub ca_exists: bool,
}

#[tauri::command]
pub fn get_cert_info(name: String) -> Result<CertInfo, String> {
    let layout = layout()?;
    let material = certificate_material(&layout, &name);
    Ok(CertInfo {
        exists: material.client_cert.is_file() && material.client_key.is_file(),
        ca_exists: material.ca_cert.is_file(),
    })
}
