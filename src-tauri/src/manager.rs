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
            let s = guard.lock().unwrap().clone(); // let-binding drops MutexGuard before guard
            s
        };

        // Fetch ccusage data
        let ccusage_result = ccusage::fetch(settings.default_history_range.max(30));
        // Fetch server rate-limit data
        let server = server_poll::fetch();

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
        state.refreshed_at = Some(now_ts());
        let payload = state.clone();
        drop(state);

        // Push to all popup windows
        let _ = app.emit("usage-updated", payload);
    });
}
