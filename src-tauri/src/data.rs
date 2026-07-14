use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── App-wide name constant ─────────────────────────────────────────────────────
pub const APP_NAME: &str = "UsageToken";

// ── User identity (read from local credentials, no extra API calls) ───────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserInfo {
    pub subscription_type: Option<String>,
    pub rate_limit_tier:   Option<String>,
}

// ── Shared state ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    pub ccusage:    Option<CcusageSnapshot>,
    pub server:     Option<ServerSnapshot>,
    pub user_info:  Option<UserInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error:      Option<String>,
    pub refreshed_at: Option<u64>,
}

// ── ccusage history data ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CcusageSnapshot {
    pub history: Vec<DailyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyEntry {
    pub period:             String,
    pub total_tokens:       u64,
    pub input_tokens:       u64,
    pub output_tokens:      u64,
    pub cache_read_tokens:  u64,
    pub cache_write_tokens: u64,
    pub cost_usd:           f64,
    pub models:             HashMap<String, ModelUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelUsage {
    pub total_tokens: u64,
    pub cost_usd:     f64,
}

// ── Server-side rate-limit data ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSnapshot {
    pub windows:          Vec<UsageWindow>,
    pub representative:   String,
    pub overall_status:   String,
    pub fetched_at:       u64,
    pub error:            Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageWindow {
    pub name:              String,
    pub utilization:       f64,
    pub percent_remaining: f64,
    pub reset_ts:          u64,
    pub status:            String,
}

// ── Settings ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub show_widget:            bool,
    pub default_history_range:  u32,
    pub launch_at_login:        bool,
    pub debounce_ms:            u64,
    pub server_poll_interval_s: u64,
    // ── Taskbar overlay ────────────────────────────────────────────────────
    pub taskbar_overlay_enabled:       bool,
    pub overlay_all_monitors_fallback: bool,
    /// Advanced: per-monitor left offset (logical px from the right edge of
    /// the taskbar), keyed by the monitor's device name (e.g. `\\.\DISPLAY1`).
    /// Monitors without an entry use the built-in default (250 on the
    /// primary monitor, 150 on secondary monitors).
    pub overlay_offset_x_overrides:    HashMap<String, i32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_widget:            false,
            default_history_range:  30,
            launch_at_login:        false,
            debounce_ms:            750,
            server_poll_interval_s: 60,
            taskbar_overlay_enabled:       false,
            overlay_all_monitors_fallback: false,
            overlay_offset_x_overrides:    HashMap::new(),
        }
    }
}
