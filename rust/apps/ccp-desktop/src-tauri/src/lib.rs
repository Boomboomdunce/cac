mod capture_manager;
mod commands;
mod mitmproxy_backend;
mod tray;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Initialize capture state
            let capture_state = capture_manager::CaptureState::new();
            let buffer = capture_state.buffer().clone();
            app.manage(capture_state);

            // Spawn event forwarder
            let handle = app.handle().clone();
            capture_manager::spawn_event_forwarder(handle, buffer);

            // Build tray
            tray::create_tray(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_setup_status,
            commands::prepare_mitm_capture,
            commands::install_mitm_trust,
            commands::remove_mitm_trust,
            commands::install_wrappers,
            commands::list_profiles,
            commands::get_profile_identity,
            commands::create_profile,
            commands::delete_profile,
            commands::switch_profile,
            commands::toggle_protection,
            commands::export_profile,
            commands::import_profile,
            commands::run_diagnostics,
            commands::get_cert_info,
            commands::test_proxy,
            commands::get_protection_layers,
            commands::get_device_identity,
            commands::update_profile,
            commands::get_global_settings,
            commands::save_global_settings,
            commands::start_capture,
            commands::stop_capture,
            commands::get_capture_status,
            commands::get_capture_snapshot,
            commands::clear_capture_buffer,
            commands::detect_egress_ip,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
