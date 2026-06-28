use crate::data::{AppState, Settings};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

type SharedState = Mutex<AppState>;
type SharedSettings = Mutex<Settings>;

#[tauri::command]
pub fn get_usage_data(state: State<'_, SharedState>) -> AppState {
    state.lock().unwrap().clone()
}

#[tauri::command]
pub fn trigger_refresh(app: AppHandle) {
    crate::manager::refresh(&app);
}

// ── Settings ──────────────────────────────────────────────────────────────────

fn settings_path(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir()
        .expect("no app data dir")
        .join("settings.json")
}

pub fn load_settings_from_disk(app: &AppHandle) -> Settings {
    let path = settings_path(app);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings_to_disk(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(state: State<'_, SharedSettings>) -> Settings {
    state.lock().unwrap().clone()
}

#[tauri::command]
pub fn show_popup(app: AppHandle, state: State<'_, SharedSettings>) {
    if let Some(w) = app.get_webview_window("popup") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let mut settings = state.lock().unwrap();
    settings.show_widget = false;
    let snapshot = settings.clone();
    drop(settings);
    if let Some(w) = app.get_webview_window("widget") {
        let _ = w.hide();
    }
    let _ = save_settings_to_disk(&app, &snapshot);
}

#[tauri::command]
pub fn toggle_widget(app: AppHandle, state: State<'_, SharedSettings>) -> bool {
    let mut settings = state.lock().unwrap();
    settings.show_widget = !settings.show_widget;
    let show = settings.show_widget;
    let snapshot = settings.clone();
    drop(settings);
    if let Some(w) = app.get_webview_window("widget") {
        if show { let _ = w.show(); } else { let _ = w.hide(); }
    }
    if show {
        if let Some(w) = app.get_webview_window("popup") {
            let _ = w.hide();
        }
    }
    let _ = save_settings_to_disk(&app, &snapshot);
    show
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, SharedSettings>,
    settings: Settings,
) -> Result<(), String> {
    // Apply autostart preference
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        let mgr = app.autolaunch();
        if settings.launch_at_login {
            let _ = mgr.enable();
        } else {
            let _ = mgr.disable();
        }
    }

    // Apply widget visibility
    let show_widget = settings.show_widget;
    if let Some(w) = app.get_webview_window("widget") {
        if show_widget {
            let _ = w.show();
        } else {
            let _ = w.hide();
        }
    }

    save_settings_to_disk(&app, &settings)?;
    *state.lock().unwrap() = settings;
    Ok(())
}
