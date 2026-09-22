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

// ── Widget positioning ──────────────────────────────────────────────────────

/// Primary monitor's work area (screen bounds minus the taskbar), in physical
/// pixels. This tracks whatever size/edge/auto-hide the user has configured
/// for their taskbar, unlike a hardcoded offset.
#[cfg(windows)]
fn primary_work_area() -> Option<windows::Win32::Foundation::RECT> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
    };
    unsafe {
        let hmonitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(hmonitor, &mut info).as_bool() {
            Some(info.rcWork)
        } else {
            None
        }
    }
}

/// Re-anchors the widget to the bottom-right corner of the primary monitor's
/// current work area. Called every time the widget is shown so it follows
/// taskbar changes (size, position, auto-hide) instead of sitting at a
/// position computed once at startup.
pub fn position_widget(app: &AppHandle) {
    let Some(w) = app.get_webview_window("widget") else { return };

    #[cfg(windows)]
    {
        let Some(work) = primary_work_area() else { return };
        let size = w
            .outer_size()
            .unwrap_or(tauri::PhysicalSize::new(300, 155));
        let margin: i32 = 12;
        let x = (work.right - size.width as i32 - margin).max(work.left);
        let y = (work.bottom - size.height as i32 - margin).max(work.top);
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

#[tauri::command]
pub fn toggle_widget(app: AppHandle, state: State<'_, SharedSettings>) -> bool {
    let mut settings = state.lock().unwrap();
    settings.show_widget = !settings.show_widget;
    let show = settings.show_widget;
    let snapshot = settings.clone();
    drop(settings);
    if show {
        position_widget(&app);
    }
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
    let old_settings = state.lock().unwrap().clone();

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
    if show_widget {
        position_widget(&app);
    }
    if let Some(w) = app.get_webview_window("widget") {
        if show_widget {
            let _ = w.show();
        } else {
            let _ = w.hide();
        }
    }

    crate::taskbar_overlay::apply_settings(&app, &old_settings, &settings);

    save_settings_to_disk(&app, &settings)?;
    *state.lock().unwrap() = settings;
    Ok(())
}
