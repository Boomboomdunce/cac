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
            let proxy_url = p.policy.proxy_url().map(ccp_core::redact_proxy_url);
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
            "redacted" => profile.policy.proxy_url().map(ccp_core::redact_proxy_url),
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

// ── Proxy Test ──

#[derive(Serialize)]
pub struct ProxyTestResult {
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub egress_ip: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn test_proxy(proxy_url: String) -> ProxyTestResult {
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    let host_port = ccp_core::proxy_host_port(&proxy_url);
    let Some(hp) = host_port else {
        return ProxyTestResult {
            reachable: false,
            latency_ms: None,
            egress_ip: None,
            error: Some("Invalid proxy URL format".to_string()),
        };
    };

    let start = Instant::now();
    match TcpStream::connect_timeout(
        &hp.parse().unwrap_or_else(|_| ([0, 0, 0, 0], 0).into()),
        Duration::from_secs(5),
    ) {
        Ok(_) => {
            let latency = start.elapsed().as_millis() as u64;
            ProxyTestResult {
                reachable: true,
                latency_ms: Some(latency),
                egress_ip: None, // Would need a real HTTP request through proxy
                error: None,
            }
        }
        Err(e) => ProxyTestResult {
            reachable: false,
            latency_ms: None,
            egress_ip: None,
            error: Some(e.to_string()),
        },
    }
}

// ── Protection Layers ──

#[derive(Serialize)]
pub struct ProtectionLayer {
    pub name: String,
    pub active: bool,
    pub description: String,
}

#[tauri::command]
pub fn get_protection_layers() -> Result<Vec<ProtectionLayer>, String> {
    let layout = layout()?;
    let runtime = RuntimeStateStore::new(layout.clone());
    let active_profile = runtime.active_profile().unwrap_or(None);
    let is_active = active_profile.is_some() && !runtime.is_paused();

    let profile_name = active_profile.as_deref().unwrap_or("");

    let has_identity = if !profile_name.is_empty() {
        load_profile_identity(&layout, profile_name).is_ok()
    } else {
        false
    };

    let has_cert = if !profile_name.is_empty() {
        let mat = certificate_material(&layout, profile_name);
        mat.client_cert.is_file() && mat.client_key.is_file()
    } else {
        false
    };

    let has_proxy = if !profile_name.is_empty() {
        let store = ProfileStore::new(layout.clone());
        store
            .load_profile(profile_name)
            .ok()
            .and_then(|p| p.policy.proxy_url().map(|_| true))
            .unwrap_or(false)
    } else {
        false
    };

    let dns_guard_exists = layout.hooks_dir().join("claude-preload.js").is_file();

    Ok(vec![
        ProtectionLayer {
            name: "proxy_injection".to_string(),
            active: is_active && has_proxy,
            description: if has_proxy {
                "HTTPS_PROXY set".to_string()
            } else {
                "No proxy configured".to_string()
            },
        },
        ProtectionLayer {
            name: "dns_telemetry_block".to_string(),
            active: is_active && dns_guard_exists,
            description: if dns_guard_exists {
                "statsig, sentry blocked".to_string()
            } else {
                "DNS guard not installed".to_string()
            },
        },
        ProtectionLayer {
            name: "env_var_protection".to_string(),
            active: is_active,
            description: if is_active {
                "12 layers injected".to_string()
            } else {
                "Inactive".to_string()
            },
        },
        ProtectionLayer {
            name: "device_identity_isolation".to_string(),
            active: is_active && has_identity,
            description: if has_identity {
                "UUID/hostname/MAC replaced".to_string()
            } else {
                "Identity not generated".to_string()
            },
        },
        ProtectionLayer {
            name: "mtls_cert_injection".to_string(),
            active: is_active && has_cert,
            description: if has_cert {
                "Client certificate valid".to_string()
            } else {
                "Certificate not generated".to_string()
            },
        },
        ProtectionLayer {
            name: "fetch_interception".to_string(),
            active: is_active && dns_guard_exists,
            description: if dns_guard_exists {
                "Native fetch patched".to_string()
            } else {
                "Preload not installed".to_string()
            },
        },
        ProtectionLayer {
            name: "ipv6_protection".to_string(),
            active: false,
            description: "Check system IPv6 settings".to_string(),
        },
    ])
}

// ── Device Identity (real values for privacy comparison) ──

#[derive(Serialize)]
pub struct DeviceIdentity {
    pub hostname: String,
    pub uuid: String,
}

#[tauri::command]
pub fn get_device_identity() -> DeviceIdentity {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    DeviceIdentity {
        hostname,
        uuid: "(protected)".to_string(),
    }
}

// ── Update Profile ──

#[derive(Deserialize)]
pub struct UpdateProfileInput {
    pub name: String,
    pub proxy_url: Option<String>,
    pub timezone: Option<String>,
    pub language: Option<String>,
}

#[tauri::command]
pub fn update_profile(input: UpdateProfileInput) -> Result<(), String> {
    let layout = layout()?;
    let store = ProfileStore::new(layout.clone());
    let mut profile = store.load_profile(&input.name).map_err(|e| e.to_string())?;

    // Rebuild policy with updated proxy
    let mut policy = PrivacyPolicy::new();
    if let Some(ref proxy) = input.proxy_url {
        if !proxy.is_empty() {
            policy = policy.with_proxy_url(proxy.clone());
        }
    }
    // Preserve blocked hosts from adapter defaults
    for host in profile.policy.blocked_hosts() {
        policy = policy.with_blocked_host(host.clone());
    }
    profile.policy = policy;

    store.save_profile(&profile).map_err(|e| e.to_string())?;

    // Update timezone/language files if provided
    let id_mat = ccp_store::identity_material(&layout, &input.name);
    if let Some(tz) = &input.timezone {
        if !tz.is_empty() {
            let _ = std::fs::write(&id_mat.tz, format!("{tz}\n"));
        }
    }
    if let Some(lang) = &input.language {
        if !lang.is_empty() {
            let _ = std::fs::write(&id_mat.lang, format!("{lang}\n"));
        }
    }

    Ok(())
}

// ── Global Settings ──

#[derive(Serialize, Deserialize)]
pub struct GlobalSettings {
    pub capture_memory_limit_mb: u64,
    pub auto_start: bool,
    pub log_level: String,
    pub language: String,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            capture_memory_limit_mb: 1024,
            auto_start: false,
            log_level: "info".to_string(),
            language: "zh-CN".to_string(),
        }
    }
}

#[tauri::command]
pub fn get_global_settings() -> Result<GlobalSettings, String> {
    let layout = layout()?;
    let path = layout.config_dir().join("gui_settings.json");
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).map_err(|e| e.to_string()),
        Err(_) => Ok(GlobalSettings::default()),
    }
}

#[tauri::command]
pub fn save_global_settings(settings: GlobalSettings) -> Result<(), String> {
    let layout = layout()?;
    let path = layout.config_dir().join("gui_settings.json");
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}
