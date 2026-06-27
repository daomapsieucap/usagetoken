use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── App-wide name constant ─────────────────────────────────────────────────────
pub const APP_NAME: &str = "UsageToken";

// ── Shared state ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    pub ccusage: Option<CcusageSnapshot>,
    pub server:  Option<ServerSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error:   Option<String>,
    pub refreshed_at: Option<u64>,
}

// ── ccusage data ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CcusageSnapshot {
    pub active_block: Option<ActiveBlock>,
    pub today:        Option<DailyEntry>,
    pub history:      Vec<DailyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveBlock {
    pub start_time:         String,
    pub end_time:           String,
    pub total_tokens:       u64,
    pub input_tokens:       u64,
    pub output_tokens:      u64,
    pub cache_read_tokens:  u64,
    pub cache_write_tokens: u64,
    pub cost_usd:           f64,
    pub usage_percent:      Option<f64>,
    pub token_limit:        Option<u64>,
    pub models:             Vec<String>,
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
pub struct Settings {
    pub show_cost:              bool,
    pub show_widget:            bool,
    pub default_history_range:  u32,
    pub launch_at_login:        bool,
    pub debounce_ms:            u64,
    pub server_poll_interval_s: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_cost:              true,
            show_widget:            false,
            default_history_range:  30,
            launch_at_login:        false,
            debounce_ms:            750,
            server_poll_interval_s: 60,
        }
    }
}
