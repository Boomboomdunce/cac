use ccp_store::{ProfileStore, RuntimeStateStore, StateLayout};
use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Manager,
};

fn state_root() -> PathBuf {
    std::env::var("CCP_STATE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".ccp-rust")
        })
}

pub fn create_tray(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let menu = build_tray_menu(app)?;

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("CCP Desktop")
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

fn build_tray_menu(app: &App) -> Result<Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    // Read current state
    let (status_label, active_profile, profiles, is_paused) = read_state();

    let status_i = MenuItem::with_id(app, "status", &status_label, false, None::<&str>)?;
    let open_i = MenuItem::with_id(app, "open", "Open Window", true, None::<&str>)?;

    // Protection toggle
    let protection_label = if is_paused {
        "Start Protection"
    } else {
        "Stop Protection"
    };
    let protection_i = MenuItem::with_id(
        app,
        "toggle_protection",
        protection_label,
        true,
        None::<&str>,
    )?;

    // Profile submenu
    let profile_sub = {
        let sub = Submenu::with_id(app, "profiles_sub", "Switch Profile", true)?;
        if profiles.is_empty() {
            let none_i =
                MenuItem::with_id(app, "no_profiles", "(no profiles)", false, None::<&str>)?;
            sub.append(&none_i)?;
        } else {
            for name in &profiles {
                let marker = if active_profile.as_deref() == Some(name.as_str()) {
                    "● "
                } else {
                    "○ "
                };
                let label = format!("{marker}{name}");
                let item_id = format!("profile_{name}");
                let item = MenuItem::with_id(app, &item_id, &label, true, None::<&str>)?;
                sub.append(&item)?;
            }
        }
        sub
    };

    let sep = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit CCP", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &status_i,
            &sep,
            &protection_i,
            &profile_sub,
            &sep2,
            &open_i,
            &sep3,
            &quit_i,
        ],
    )?;

    Ok(menu)
}

fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id.as_ref();

    match id {
        "quit" => {
            app.exit(0);
        }
        "open" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "toggle_protection" => {
            if let Ok(layout) = StateLayout::new(state_root()) {
                let runtime = RuntimeStateStore::new(layout);
                let currently_paused = runtime.is_paused();
                let _ = runtime.set_paused(!currently_paused);
            }
        }
        _ if id.starts_with("profile_") => {
            let profile_name = &id["profile_".len()..];
            if let Ok(layout) = StateLayout::new(state_root()) {
                let runtime = RuntimeStateStore::new(layout);
                let _ = runtime.set_active_profile(profile_name);
                let _ = runtime.set_paused(false);
            }
        }
        _ => {}
    }
}

fn read_state() -> (String, Option<String>, Vec<String>, bool) {
    let Ok(layout) = StateLayout::new(state_root()) else {
        return ("CCP Desktop".to_string(), None, vec![], false);
    };

    let runtime = RuntimeStateStore::new(layout.clone());
    let active = runtime.active_profile().unwrap_or(None);
    let paused = runtime.is_paused();

    let status_label = match (&active, paused) {
        (Some(name), false) => format!("● Running · {name}"),
        (Some(name), true) => format!("■ Paused · {name}"),
        (None, _) => "○ No profile active".to_string(),
    };

    let store = ProfileStore::new(layout);
    let profiles = store
        .list_profiles()
        .map(|ps| ps.into_iter().map(|p| p.name).collect())
        .unwrap_or_default();

    (status_label, active, profiles, paused)
}
