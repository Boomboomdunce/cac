use serde::Serialize;

#[derive(Serialize)]
pub struct AppStatus {
    pub active: bool,
    pub profile: Option<String>,
    pub version: String,
}

#[tauri::command]
pub fn get_status() -> AppStatus {
    AppStatus {
        active: false,
        profile: None,
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[derive(Serialize)]
pub struct ProfileSummary {
    pub name: String,
    pub active: bool,
}

#[tauri::command]
pub fn list_profiles() -> Vec<ProfileSummary> {
    vec![]
}
