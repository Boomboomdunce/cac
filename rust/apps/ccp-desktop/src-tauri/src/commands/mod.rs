use ccp::{default_state_root, inspect_setup_status, install};
use ccp_core::{PrivacyPolicy, Profile};
use ccp_store::{
    certificate_material, ensure_mitm_certificates, ensure_profile_certificates,
    ensure_profile_identity_seeded, install_mitm_system_trust, load_profile_identity,
    mitm_certificate_material, mitm_system_trust_status, remove_mitm_system_trust,
    MitmSystemTrustStatus, ProfileStore, RuntimeStateStore, StateLayout,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(crate) fn state_root() -> PathBuf {
    default_state_root().unwrap_or_else(|_| {
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

#[derive(Serialize)]
pub struct SetupStatus {
    pub state_root: String,
    pub wrappers_installed: bool,
    pub can_auto_install_wrappers: bool,
    pub install_metadata_present: bool,
    pub install_command: String,
    pub suggested_bin_dir: String,
    pub suggested_shell_rc: Option<String>,
    pub profiles: Vec<String>,
    pub active_profile: Option<String>,
    pub active_profile_has_proxy: bool,
    pub proxy_required_for_capture: bool,
    pub capture_backend_mode: String,
    pub transparent_capture_available: bool,
    pub transparent_capture_status: String,
    pub mitm_ready: bool,
    pub mitm_status: String,
    pub mitm_system_trust_supported: bool,
    pub mitm_system_trust_installed: bool,
    pub mitm_system_trust_status: String,
}

#[derive(Deserialize)]
pub struct InstallWrappersInput {
    pub update_shell_rc: bool,
}

#[derive(Serialize)]
pub struct InstallWrappersResult {
    pub bin_dir: String,
    pub shell_rc: Option<String>,
    pub ccp_bin_path: String,
    pub generated_paths: Vec<String>,
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

#[tauri::command]
pub fn get_setup_status() -> Result<SetupStatus, String> {
    setup_status_for_root(&state_root())
}

fn setup_status_for_root(root: &std::path::Path) -> Result<SetupStatus, String> {
    let status = inspect_setup_status(root);
    let layout = StateLayout::new(root).map_err(|e| e.to_string())?;
    let settings = load_global_settings_from_root(root);
    let transparent = crate::mitmproxy_backend::inspect_transparent_capture_support();
    let proxy_required_for_capture =
        !capture_mode_prefers_transparent(&settings.capture_backend_mode) || !transparent.available;
    let mitm_material = mitm_certificate_material(&layout);
    let mitm_ready = mitm_material.ca_cert.is_file()
        && mitm_material.ca_key.is_file()
        && mitm_material.node_ca_bundle.is_file();
    let mitm_status = if mitm_ready {
        "MITM capture certificates ready".to_string()
    } else {
        "Missing MITM capture certificates".to_string()
    };
    let trust =
        mitm_system_trust_status(&layout).unwrap_or_else(|err| ccp_store::MitmSystemTrustStatus {
            supported: cfg!(target_os = "macos"),
            installed: false,
            keychain: None,
            message: format!("MITM system trust check failed: {err}"),
        });

    Ok(SetupStatus {
        state_root: status.state_root.display().to_string(),
        wrappers_installed: status.wrappers_installed,
        can_auto_install_wrappers: status.ccp_binary_path.is_some(),
        install_metadata_present: status.install_metadata_present,
        install_command: "ccp setup".to_string(),
        suggested_bin_dir: status.suggested_bin_dir.display().to_string(),
        suggested_shell_rc: status
            .suggested_shell_rc
            .as_ref()
            .map(|path| path.display().to_string()),
        profiles: status.profiles,
        active_profile: status.active_profile,
        active_profile_has_proxy: status.active_profile_has_proxy,
        proxy_required_for_capture,
        capture_backend_mode: settings.capture_backend_mode,
        transparent_capture_available: transparent.available,
        transparent_capture_status: transparent.message,
        mitm_ready,
        mitm_status,
        mitm_system_trust_supported: trust.supported,
        mitm_system_trust_installed: trust.installed,
        mitm_system_trust_status: trust.message,
    })
}

fn prepare_mitm_capture_for_layout(
    layout: &StateLayout,
    active_profile: Option<&str>,
) -> Result<(), String> {
    if let Some(profile_name) = active_profile {
        ensure_profile_certificates(layout, profile_name).map_err(|e| e.to_string())?;
    }
    ensure_mitm_certificates(layout).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn prepare_mitm_capture() -> Result<(), String> {
    let layout = layout()?;
    let runtime = RuntimeStateStore::new(layout.clone());
    let active_profile = runtime.active_profile().map_err(|e| e.to_string())?;
    prepare_mitm_capture_for_layout(&layout, active_profile.as_deref())
}

#[tauri::command]
pub fn install_mitm_trust() -> Result<String, String> {
    let layout = layout()?;
    let status = install_mitm_system_trust(&layout).map_err(|e| e.to_string())?;
    Ok(status.message)
}

#[tauri::command]
pub fn remove_mitm_trust() -> Result<String, String> {
    let layout = layout()?;
    let status = remove_mitm_system_trust(&layout).map_err(|e| e.to_string())?;
    Ok(status.message)
}

#[tauri::command]
pub fn install_wrappers(input: InstallWrappersInput) -> Result<InstallWrappersResult, String> {
    let root = state_root();
    let layout = StateLayout::new(root).map_err(|e| e.to_string())?;
    let status = inspect_setup_status(layout.root());
    let ccp_bin_path = status.ccp_binary_path.ok_or_else(|| {
        "Could not locate a runnable ccp binary. Build or install ccp first, then retry."
            .to_string()
    })?;
    let shell_rc = if input.update_shell_rc {
        status.suggested_shell_rc.clone()
    } else {
        None
    };

    let metadata = install::setup(
        &layout,
        install::SetupConfig {
            bin_dir: status.suggested_bin_dir,
            shell_rc,
            ccp_bin_path: ccp_bin_path.clone(),
        },
    )
    .map_err(|e| e.to_string())?;

    Ok(InstallWrappersResult {
        bin_dir: metadata.bin_dir.display().to_string(),
        shell_rc: metadata.shell_rc.map(|path| path.display().to_string()),
        ccp_bin_path: ccp_bin_path.display().to_string(),
        generated_paths: metadata
            .generated_paths
            .into_iter()
            .map(|path| path.display().to_string())
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        prepare_mitm_capture_for_layout, setup_status_for_root, update_profile, GlobalSettings,
        UpdateProfileInput,
    };
    use ccp_core::{PrivacyPolicy, Profile};
    use ccp_store::{ProfileStore, StateLayout};
    use tempfile::tempdir;

    #[test]
    fn setup_status_reports_missing_mitm_assets_until_prepared() {
        let root = std::env::temp_dir().join(format!(
            "ccp-desktop-mitm-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let status = setup_status_for_root(&root).unwrap();
        assert!(!status.mitm_ready);
        assert!(status.mitm_status.contains("Missing"));
        assert!(
            status.mitm_system_trust_status.contains("not supported")
                || status.mitm_system_trust_status.contains("not trusted")
                || status
                    .mitm_system_trust_status
                    .contains("not been prepared")
        );

        let layout = StateLayout::new(&root).unwrap();
        prepare_mitm_capture_for_layout(&layout, None).unwrap();

        let status = setup_status_for_root(&root).unwrap();
        assert!(status.mitm_ready);
        assert!(status.mitm_status.contains("ready"));
        assert!(
            status.mitm_system_trust_supported
                || status.mitm_system_trust_status.contains("not supported")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn update_profile_persists_proxy_url() {
        let temp = tempdir().unwrap();
        let layout = StateLayout::new(temp.path()).unwrap();
        let store = ProfileStore::new(layout.clone());
        let profile = Profile::new("work", "claude", PrivacyPolicy::new());
        store.save_profile(&profile).unwrap();

        let previous = std::env::var_os("CCP_STATE_ROOT");
        std::env::set_var("CCP_STATE_ROOT", temp.path());

        update_profile(UpdateProfileInput {
            name: "work".to_string(),
            proxy_url: Some("http://127.0.0.1:6152".to_string()),
            timezone: None,
            language: None,
        })
        .unwrap();

        let updated = store.load_profile("work").unwrap();
        assert_eq!(updated.policy.proxy_url(), Some("http://127.0.0.1:6152"));

        if let Some(value) = previous {
            std::env::set_var("CCP_STATE_ROOT", value);
        } else {
            std::env::remove_var("CCP_STATE_ROOT");
        }
    }

    #[test]
    fn global_settings_default_capture_backend_mode_is_auto() {
        let settings = GlobalSettings::default();
        assert_eq!(settings.capture_backend_mode, "auto");
    }

    #[test]
    fn setup_status_marks_proxy_optional_when_transparent_capture_is_available() {
        let _guard = crate::mitmproxy_backend::TEST_ENV_LOCK.lock().unwrap();
        let temp = tempdir().unwrap();
        let layout = StateLayout::new(temp.path()).unwrap();
        let store = ProfileStore::new(layout.clone());
        let profile = Profile::new("work", "claude", PrivacyPolicy::new());
        store.save_profile(&profile).unwrap();
        ccp_store::RuntimeStateStore::new(layout)
            .set_active_profile("work")
            .unwrap();

        let fake_mitmdump = temp.path().join("mitmdump");
        std::fs::write(&fake_mitmdump, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&fake_mitmdump).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&fake_mitmdump, perms).unwrap();
        }

        let previous_root = std::env::var_os("CCP_STATE_ROOT");
        let previous_mitmdump = std::env::var_os("CCP_MITMDUMP_PATH");
        #[cfg(target_os = "macos")]
        let previous_systemextensions = std::env::var_os("CCP_TEST_SYSTEMEXTENSIONSCTL_LIST");
        std::env::set_var("CCP_STATE_ROOT", temp.path());
        std::env::set_var("CCP_MITMDUMP_PATH", &fake_mitmdump);
        #[cfg(target_os = "macos")]
        std::env::set_var(
            "CCP_TEST_SYSTEMEXTENSIONSCTL_LIST",
            r#"
--- com.apple.system_extension.network_extension
enabled	active	teamID	bundleID (version)	name	[state]
*	*	S8XHQB96PW	org.mitmproxy.macos-redirector.network-extension (2.0/1)	network-extension	[activated enabled]
"#,
        );

        let status = setup_status_for_root(temp.path()).unwrap();
        assert!(status.transparent_capture_available);
        assert!(!status.proxy_required_for_capture);

        if let Some(value) = previous_root {
            std::env::set_var("CCP_STATE_ROOT", value);
        } else {
            std::env::remove_var("CCP_STATE_ROOT");
        }

        if let Some(value) = previous_mitmdump {
            std::env::set_var("CCP_MITMDUMP_PATH", value);
        } else {
            std::env::remove_var("CCP_MITMDUMP_PATH");
        }

        #[cfg(target_os = "macos")]
        if let Some(value) = previous_systemextensions {
            std::env::set_var("CCP_TEST_SYSTEMEXTENSIONSCTL_LIST", value);
        } else {
            std::env::remove_var("CCP_TEST_SYSTEMEXTENSIONSCTL_LIST");
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn setup_status_marks_transparent_unavailable_when_redirector_waits_for_user() {
        let _guard = crate::mitmproxy_backend::TEST_ENV_LOCK.lock().unwrap();
        let temp = tempdir().unwrap();
        let layout = StateLayout::new(temp.path()).unwrap();
        let store = ProfileStore::new(layout.clone());
        let profile = Profile::new("work", "claude", PrivacyPolicy::new());
        store.save_profile(&profile).unwrap();
        ccp_store::RuntimeStateStore::new(layout)
            .set_active_profile("work")
            .unwrap();

        let fake_mitmdump = temp.path().join("mitmdump");
        std::fs::write(&fake_mitmdump, "#!/bin/sh\nexit 0\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&fake_mitmdump).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_mitmdump, perms).unwrap();

        let previous_root = std::env::var_os("CCP_STATE_ROOT");
        let previous_mitmdump = std::env::var_os("CCP_MITMDUMP_PATH");
        let previous_systemextensions = std::env::var_os("CCP_TEST_SYSTEMEXTENSIONSCTL_LIST");
        std::env::set_var("CCP_STATE_ROOT", temp.path());
        std::env::set_var("CCP_MITMDUMP_PATH", &fake_mitmdump);
        std::env::set_var(
            "CCP_TEST_SYSTEMEXTENSIONSCTL_LIST",
            r#"
--- com.apple.system_extension.network_extension
enabled	active	teamID	bundleID (version)	name	[state]
	*	S8XHQB96PW	org.mitmproxy.macos-redirector.network-extension (2.0/1)	network-extension	[activated waiting for user]
"#,
        );

        let status = setup_status_for_root(temp.path()).unwrap();
        assert!(!status.transparent_capture_available);
        assert!(status.proxy_required_for_capture);
        assert!(status.transparent_capture_status.contains("waiting for approval"));

        if let Some(value) = previous_root {
            std::env::set_var("CCP_STATE_ROOT", value);
        } else {
            std::env::remove_var("CCP_STATE_ROOT");
        }

        if let Some(value) = previous_mitmdump {
            std::env::set_var("CCP_MITMDUMP_PATH", value);
        } else {
            std::env::remove_var("CCP_MITMDUMP_PATH");
        }

        if let Some(value) = previous_systemextensions {
            std::env::set_var("CCP_TEST_SYSTEMEXTENSIONSCTL_LIST", value);
        } else {
            std::env::remove_var("CCP_TEST_SYSTEMEXTENSIONSCTL_LIST");
        }
    }
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
    runtime
        .set_active_profile(&name)
        .map_err(|e| e.to_string())?;
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
    let root = state_root();
    let config = ccp_doctor::DoctorConfig::new(root.clone(), profile_name.clone());
    let mut report = ccp_doctor::run(config);
    ccp::audit::augment_doctor_report_with_live_runtime_audit(&mut report, &root, &profile_name);

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
    let mitm_material = mitm_certificate_material(&layout);
    let mitm_ready = mitm_material.ca_cert.is_file()
        && mitm_material.ca_key.is_file()
        && mitm_material.node_ca_bundle.is_file();
    let trust = mitm_system_trust_status(&layout).unwrap_or_else(|err| MitmSystemTrustStatus {
        supported: cfg!(target_os = "macos"),
        installed: false,
        keychain: None,
        message: format!("MITM system trust check failed: {err}"),
    });

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
            name: "https_mitm_capture".to_string(),
            active: is_active && mitm_ready,
            description: if mitm_ready {
                "HTTPS MITM certificates ready".to_string()
            } else {
                "MITM certificates missing".to_string()
            },
        },
        ProtectionLayer {
            name: "system_cert_trust".to_string(),
            active: trust.installed,
            description: if trust.supported {
                trust.message
            } else {
                "System trust installation is optional and unsupported here".to_string()
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
    pub capture_backend_mode: String,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            capture_memory_limit_mb: 1024,
            auto_start: false,
            log_level: "info".to_string(),
            language: "zh-CN".to_string(),
            capture_backend_mode: "auto".to_string(),
        }
    }
}

fn load_global_settings_from_root(root: &std::path::Path) -> GlobalSettings {
    let Ok(layout) = StateLayout::new(root) else {
        return GlobalSettings::default();
    };
    let path = layout.config_dir().join("gui_settings.json");
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => GlobalSettings::default(),
    }
}

fn capture_mode_prefers_transparent(mode: &str) -> bool {
    matches!(mode, "auto" | "transparent")
}

#[tauri::command]
pub fn get_global_settings() -> Result<GlobalSettings, String> {
    Ok(load_global_settings_from_root(&state_root()))
}

#[tauri::command]
pub fn save_global_settings(settings: GlobalSettings) -> Result<(), String> {
    let layout = layout()?;
    let path = layout.config_dir().join("gui_settings.json");
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Capture Proxy ──

#[tauri::command]
pub async fn start_capture(
    state: tauri::State<'_, crate::capture_manager::CaptureState>,
) -> Result<CaptureStatus, String> {
    let lay = layout()?;
    let runtime = RuntimeStateStore::new(lay.clone());
    let profile_name = runtime
        .active_profile()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No active profile".to_string())?;
    let settings = load_global_settings_from_root(lay.root());
    let transparent = crate::mitmproxy_backend::inspect_transparent_capture_support();

    let store = ProfileStore::new(lay);
    let profile = store
        .load_profile(&profile_name)
        .map_err(|e| e.to_string())?;

    let lay = layout()?;
    prepare_mitm_capture_for_layout(&lay, Some(&profile_name))?;

    if capture_mode_prefers_transparent(&settings.capture_backend_mode) {
        if transparent.available {
            let status = state
                .start_transparent_capture(&lay, "all", "claude")
                .await?;
            return Ok(CaptureStatus::from_runtime(status));
        }
        if settings.capture_backend_mode == "transparent" {
            return Err(format!(
                "Transparent capture is selected, but {}",
                transparent.message
            ));
        }
    }

    let proxy_url = profile.policy.proxy_url().ok_or_else(|| {
        if capture_mode_prefers_transparent(&settings.capture_backend_mode) {
            format!(
                "Transparent capture is not ready: {} Active profile has no upstream proxy configured for explicit fallback.",
                transparent.message
            )
        } else {
            "Active profile has no proxy configured".to_string()
        }
    })?;
    let upstream =
        ccp_core::proxy_host_port(proxy_url).ok_or_else(|| "Cannot parse proxy URL".to_string())?;
    let mitm_material = ensure_mitm_certificates(&lay).map_err(|e| e.to_string())?;
    let mitm = ccp_sidecar::MitmProxyConfig {
        ca_cert_pem: std::fs::read_to_string(&mitm_material.ca_cert).map_err(|e| e.to_string())?,
        ca_key_pem: std::fs::read_to_string(&mitm_material.ca_key).map_err(|e| e.to_string())?,
        upstream_ca_cert_pem: None,
        max_body_bytes: 64 * 1024,
    };

    let status = state
        .start_explicit_proxy(upstream, "claude".to_string(), Some(mitm), state_root())
        .await?;
    Ok(CaptureStatus::from_runtime(status))
}

#[tauri::command]
pub async fn stop_capture(
    state: tauri::State<'_, crate::capture_manager::CaptureState>,
) -> Result<(), String> {
    state.stop_capture(state_root()).await;
    Ok(())
}

#[tauri::command]
pub async fn get_capture_status(
    state: tauri::State<'_, crate::capture_manager::CaptureState>,
) -> Result<CaptureStatus, String> {
    Ok(CaptureStatus::from_runtime(state.status().await))
}

#[derive(Serialize)]
pub struct CaptureStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub backend: String,
    pub target: Option<String>,
    pub warning: Option<String>,
}

impl CaptureStatus {
    fn from_runtime(status: crate::capture_manager::CaptureRuntimeStatus) -> Self {
        Self {
            running: status.running,
            port: status.port,
            backend: status.backend,
            target: status.target,
            warning: status.warning,
        }
    }
}

#[tauri::command]
pub fn get_capture_snapshot(
    state: tauri::State<'_, crate::capture_manager::CaptureState>,
) -> Vec<ccp_sidecar::CapturedRequest> {
    state.buffer().snapshot()
}

#[tauri::command]
pub fn clear_capture_buffer(state: tauri::State<'_, crate::capture_manager::CaptureState>) {
    state.buffer().clear();
}

// ── Egress IP ──

#[tauri::command]
pub async fn detect_egress_ip() -> Result<String, String> {
    let lay = layout().map_err(|e| e.to_string())?;
    let runtime = RuntimeStateStore::new(lay.clone());
    let profile_name = runtime
        .active_profile()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No active profile".to_string())?;
    let store = ProfileStore::new(lay);
    let profile = store
        .load_profile(&profile_name)
        .map_err(|e| e.to_string())?;
    let proxy_url = profile
        .policy
        .proxy_url()
        .ok_or_else(|| "No proxy configured".to_string())?;
    let proxy_addr =
        ccp_core::proxy_host_port(proxy_url).ok_or_else(|| "Cannot parse proxy URL".to_string())?;

    ccp_sidecar::detect_egress_ip(&proxy_addr).await
}
