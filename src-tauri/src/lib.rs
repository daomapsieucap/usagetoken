pub mod ccusage;
pub mod commands;
pub mod data;
pub mod manager;
pub mod server_poll;
pub mod soak;
pub mod tray;
pub mod watcher;

use data::{AppState, Settings};
use std::{path::PathBuf, sync::Mutex, time::Duration};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(Mutex::new(AppState::default()))
        .manage(Mutex::new(Settings::default()))
        .setup(|app| {
            // ── Dev-only soak-test memory logger (UT_SOAK_LOG=1) ────────────────
            soak::init();

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
            .inner_size(300.0, 155.0)
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
            // Ticks every second (instead of one long sleep(poll_secs)) so a
            // laptop sleep/resume shows up as a wall-clock jump we can react to
            // immediately, rather than waiting out a stale sleep after waking.
            let poll_secs = settings.server_poll_interval_s;
            let app_h = app.handle().clone();
            std::thread::spawn(move || {
                let mut last_wall = std::time::SystemTime::now();
                let mut last_refresh = std::time::Instant::now();
                loop {
                    std::thread::sleep(Duration::from_secs(1));

                    let now_wall = std::time::SystemTime::now();
                    let wall_elapsed = now_wall
                        .duration_since(last_wall)
                        .unwrap_or(Duration::from_secs(1));
                    last_wall = now_wall;

                    // A ~1s tick taking much longer than that in wall-clock time
                    // means the process (and likely the whole machine) was
                    // suspended in between - refresh right away instead of
                    // waiting for the rest of the old interval to elapse.
                    let woke_from_sleep = wall_elapsed > Duration::from_secs(5);

                    if woke_from_sleep || last_refresh.elapsed() >= Duration::from_secs(poll_secs) {
                        manager::refresh(&app_h);
                        last_refresh = std::time::Instant::now();
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_usage_data,
            commands::trigger_refresh,
            commands::get_settings,
            commands::save_settings,
            commands::toggle_widget,
            commands::show_popup,
            soak::soak_enabled,
            soak::soak_log,
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

