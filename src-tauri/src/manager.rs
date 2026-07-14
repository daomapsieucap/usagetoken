use crate::{ccusage, data::AppState, server_poll};
use std::{
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};

fn now_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn refresh(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let settings = {
            let guard = app.state::<Mutex<crate::data::Settings>>();
            let s = guard.lock().unwrap().clone();
            s
        };

        let ccusage_result = ccusage::fetch(&app, settings.default_history_range.max(30));
        let server    = server_poll::fetch();
        let user_info = server_poll::read_user_info();

        let state_guard = app.state::<Mutex<AppState>>();
        let mut state = state_guard.lock().unwrap();
        match ccusage_result {
            Ok(snap) => {
                state.ccusage = Some(snap);
                state.error   = None;
            }
            Err(e) => {
                state.error = Some(e);
            }
        }
        state.server       = Some(server);
        state.user_info    = Some(user_info);
        state.refreshed_at = Some(now_ts());
        let payload = state.clone();
        drop(state);

        update_taskbar_overlay(&payload);

        let _ = app.emit("usage-updated", payload);
    });
}

// Overlay-specific staleness guard: no fresh data, a fetch error, or the last
// successful refresh being old enough that the numbers can no longer be
// trusted at a glance (3x the default poll interval).
const OVERLAY_STALE_AFTER_SECS: u64 = 180;

fn update_taskbar_overlay(state: &AppState) {
    let stale = state.error.is_some()
        || state.server.as_ref().map(|s| s.error.is_some()).unwrap_or(true)
        || state
            .refreshed_at
            .map(|t| now_ts().saturating_sub(t) > OVERLAY_STALE_AFTER_SECS)
            .unwrap_or(true);

    let pct_of = |name: &str| -> u8 {
        state
            .server
            .as_ref()
            .and_then(|s| s.windows.iter().find(|w| w.name == name))
            .map(|w| (w.utilization * 100.0).round().clamp(0.0, 100.0) as u8)
            .unwrap_or(0)
    };

    crate::taskbar_overlay::update(pct_of("5h"), pct_of("7d"), stale);
}
