pub mod ccusage;
pub mod commands;
pub mod data;
pub mod manager;
pub mod server_poll;
pub mod tray;
pub mod watcher;

use data::{AppState, Settings};
use std::{path::PathBuf, sync::Mutex, time::Duration};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(Mutex::new(AppState::default()))
        .manage(Mutex::new(Settings::default()))
        .setup(|app| {
            // ── Load settings from disk ──────────────────────────────────────
            let settings = commands::load_settings_from_disk(&app.handle());
            *app.state::<Mutex<Settings>>().lock().unwrap() = settings.clone();

            // ── Popup window ──────────────────────────────────────────────────
            WebviewWindowBuilder::new(
                app,
                "popup",
                WebviewUrl::App("index.html".into()),
            )
            .title(data::APP_NAME)
            .inner_size(480.0, 640.0)
            .min_inner_size(400.0, 500.0)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .build()?;

            // ── Widget window ─────────────────────────────────────────────────
            let widget = WebviewWindowBuilder::new(
                app,
                "widget",
                WebviewUrl::App("widget.html".into()),
            )
            .title(format!("{} Widget", data::APP_NAME))
            .inner_size(220.0, 90.0)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .build()?;

            if settings.show_widget {
                let _ = widget.show();
            }

            // ── System tray ───────────────────────────────────────────────────
            tray::setup(app.handle())?;

            // ── Initial data fetch ────────────────────────────────────────────
            manager::refresh(app.handle());

            // ── File watcher on ~/.claude/projects/ ───────────────────────────
            let claude_dir = claude_projects_dir();
            if claude_dir.exists() {
                let debounce = Duration::from_millis(settings.debounce_ms);
                let app_h = app.handle().clone();
                watcher::watch(claude_dir, debounce, move || {
                    manager::refresh(&app_h);
                });
            }

            // ── Periodic server poll ──────────────────────────────────────────
            let poll_secs = settings.server_poll_interval_s;
            let app_h = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(poll_secs));
                manager::refresh(&app_h);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_usage_data,
            commands::trigger_refresh,
            commands::load_notes,
            commands::save_notes,
            commands::get_settings,
            commands::save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running UsageToken");
}

fn claude_projects_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        base.join(".claude").join("projects")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let base = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        base.join(".claude").join("projects")
    }
}
