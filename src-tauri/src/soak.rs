use std::{
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

static ENABLED: AtomicBool = AtomicBool::new(false);

// Dev-only soak-test logger. Off unless UT_SOAK_LOG=1 is set before launch;
// the frontend polls soak_enabled() once and only then starts its 5 minute
// timer that calls soak_log() with the JS heap size.
pub fn init() {
    let on = std::env::var("UT_SOAK_LOG").map(|v| v == "1").unwrap_or(false);
    ENABLED.store(on, Ordering::Relaxed);
}

#[tauri::command]
pub fn soak_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

fn log_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|p| p.join("soak.log"))
}

#[cfg(windows)]
fn working_set_bytes() -> u64 {
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) != 0 {
            counters.WorkingSetSize as u64
        } else {
            0
        }
    }
}

#[cfg(not(windows))]
fn working_set_bytes() -> u64 {
    0
}

#[tauri::command]
pub fn soak_log(app: AppHandle, js_heap_bytes: u64) {
    if !soak_enabled() {
        return;
    }
    let Some(path) = log_path(&app) else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let line = format!(
        "ts={ts} working_set_bytes={} js_heap_bytes={js_heap_bytes}\n",
        working_set_bytes()
    );
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
}
