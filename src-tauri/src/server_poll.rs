use crate::data::{ServerSnapshot, UsageWindow, UserInfo};
use std::{path::PathBuf, sync::OnceLock, time::{SystemTime, UNIX_EPOCH}};

const POLL_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const POLL_MODEL:    &str = "claude-haiku-4-5-20251001";
const POLL_API_VER:  &str = "2023-06-01";

fn now_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

// Built once and reused so the connection pool survives across polls
// instead of re-handshaking TLS every time.
fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(ureq::Agent::new)
}

fn credentials_path() -> PathBuf {
    let home = dirs_home();
    home.join(".claude").join(".credentials.json")
}

fn dirs_home() -> PathBuf {
    // Use USERPROFILE on Windows, HOME elsewhere
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOMEDRIVE").and_then(|d| std::env::var("HOMEPATH").map(|p| d + &p)))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."))
    }
}

// ── Read user identity from local credentials file (no network) ───────────────

fn format_tier(raw: &str) -> String {
    // "default_claude_ai" → "claude.ai", "high_utilization" → "high utilization", etc.
    let s = raw.trim_start_matches("default_");
    s.replace('_', ".")
}

pub fn read_user_info() -> UserInfo {
    let path = credentials_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return UserInfo::default(),
    };
    let val: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return UserInfo::default(),
    };
    let oauth = match val.get("claudeAiOauth") {
        Some(o) => o,
        None => return UserInfo::default(),
    };

    let subscription_type = oauth.get("subscriptionType")
        .and_then(|v| v.as_str())
        .map(String::from);

    let rate_limit_tier = oauth.get("rateLimitTier")
        .and_then(|v| v.as_str())
        .map(|s| format_tier(s));

    UserInfo { subscription_type, rate_limit_tier }
}

fn get_oauth_token() -> Option<String> {
    if let Ok(t) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        if !t.is_empty() {
            return Some(t);
        }
    }
    let path = credentials_path();
    let text = std::fs::read_to_string(&path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&text).ok()?;
    val.get("claudeAiOauth")
        .and_then(|o| o.get("accessToken"))
        .and_then(|t| t.as_str())
        .map(String::from)
}

fn parse_headers(hdrs: &[(String, String)]) -> ServerSnapshot {
    let get = |key: &str| -> Option<String> {
        hdrs.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.clone())
    };
    let get_f64 = |key: &str| -> Option<f64> {
        get(key).and_then(|v| v.parse().ok())
    };
    let get_u64 = |key: &str| -> Option<u64> {
        get(key).and_then(|v| v.parse::<f64>().ok()).map(|f| f as u64)
    };

    let mut windows = Vec::new();
    for win in &["5h", "7d"] {
        if let Some(util) = get_f64(&format!("anthropic-ratelimit-unified-{}-utilization", win)) {
            let util = util.clamp(0.0, 1.0);
            windows.push(UsageWindow {
                name:              win.to_string(),
                utilization:       util,
                percent_remaining: ((1.0 - util) * 100.0).max(0.0),
                reset_ts:          get_u64(&format!("anthropic-ratelimit-unified-{}-reset", win)).unwrap_or(0),
                status:            get(&format!("anthropic-ratelimit-unified-{}-status", win)).unwrap_or_else(|| "unknown".into()),
            });
        }
    }

    let representative = get("anthropic-ratelimit-unified-representative-claim").unwrap_or_else(|| "five_hour".into());
    let overall_status  = get("anthropic-ratelimit-unified-status").unwrap_or_else(|| "unknown".into());
    let error = if windows.is_empty() {
        Some("No unified rate-limit headers in response (API format may have changed)".into())
    } else {
        None
    };

    ServerSnapshot { windows, representative, overall_status, fetched_at: now_ts(), error }
}

pub fn fetch() -> ServerSnapshot {
    if POLL_ENDPOINT.is_empty() {
        return ServerSnapshot {
            windows: vec![], representative: "five_hour".into(),
            overall_status: "unknown".into(), fetched_at: now_ts(),
            error: Some("Server polling disabled".into()),
        };
    }

    let token = match get_oauth_token() {
        Some(t) => t,
        None => return ServerSnapshot {
            windows: vec![], representative: "five_hour".into(),
            overall_status: "unknown".into(), fetched_at: now_ts(),
            error: Some("OAuth token not found — run /login in Claude Code".into()),
        },
    };

    let body = serde_json::json!({
        "model": POLL_MODEL,
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "h"}]
    }).to_string();

    let result = agent()
        .post(POLL_ENDPOINT)
        .set("Authorization", &format!("Bearer {}", token))
        .set("Content-Type", "application/json")
        .set("anthropic-version", POLL_API_VER)
        .set("User-Agent", "UsageToken/0.1")
        .send_string(&body);

    // Extract the response (or an HTTP-error response) to read headers from it.
    let (resp_opt, transport_err) = match result {
        Ok(resp)                        => (Some(resp), None),
        Err(ureq::Error::Status(_, r))  => (Some(r),    None),
        Err(e)                          => (None,        Some(e.to_string())),
    };

    let Some(resp) = resp_opt else {
        return ServerSnapshot {
            windows: vec![], representative: "five_hour".into(),
            overall_status: "unknown".into(), fetched_at: now_ts(),
            error: transport_err,
        };
    };

    let header_names = [
        "anthropic-ratelimit-unified-5h-utilization",
        "anthropic-ratelimit-unified-5h-reset",
        "anthropic-ratelimit-unified-5h-status",
        "anthropic-ratelimit-unified-7d-utilization",
        "anthropic-ratelimit-unified-7d-reset",
        "anthropic-ratelimit-unified-7d-status",
        "anthropic-ratelimit-unified-representative-claim",
        "anthropic-ratelimit-unified-status",
    ];
    let headers: Vec<(String, String)> = header_names
        .iter()
        .filter_map(|name| resp.header(name).map(|v| (name.to_string(), v.to_string())))
        .collect();

    let mut snap = parse_headers(&headers);
    if snap.error.is_some() {
        let status = resp.status();
        if status == 401 {
            snap.error = Some("401 — token expired; run `claude setup-token` or /login".into());
        } else if status == 429 {
            snap.error = Some("429 — already rate-limited".into());
        } else if status >= 400 {
            snap.error = Some(format!("HTTP {status}"));
        }
    }
    snap
}
