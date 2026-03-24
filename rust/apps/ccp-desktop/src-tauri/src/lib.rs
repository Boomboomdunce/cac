mod commands;
mod tray;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
