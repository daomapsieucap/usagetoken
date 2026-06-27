use crate::data::{ActiveBlock, CcusageSnapshot, DailyEntry, ModelUsage};
use chrono::{Local, Duration};
use serde_json::Value;
use std::{collections::HashMap, process::Command};

#[cfg(target_os = "windows")]
fn no_window() -> u32 {
    0x08000000 // CREATE_NO_WINDOW
}
#[cfg(not(target_os = "windows"))]
fn no_window() -> u32 {
    0
}

fn run_ccusage(args: &[&str]) -> Option<Value> {
    let candidates: &[&str] = &["ccusage", "ccusage.cmd"];
    for bin in candidates {
        let mut cmd = Command::new(bin);
        cmd.args(args);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(no_window());
        }
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                if let Ok(s) = std::str::from_utf8(&out.stdout) {
                    if let Ok(v) = serde_json::from_str::<Value>(s.trim()) {
                        return Some(v);
                    }
                }
            }
        }
    }
    None
}

pub fn is_available() -> bool {
    let candidates: &[&str] = &["ccusage", "ccusage.cmd"];
    for bin in candidates {
        let mut cmd = Command::new(bin);
        cmd.arg("--version");
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(no_window());
        }
        if cmd.output().map(|o| o.status.success()).unwrap_or(false) {
            return true;
        }
    }
    false
}

fn since_date(days: u32) -> String {
    let dt = Local::now() - Duration::days(days as i64);
    dt.format("%Y-%m-%d").to_string()
}

fn today_str() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn parse_u64(v: &Value) -> u64 {
    match v {
        Value::Number(n) => n.as_u64().unwrap_or(0),
        Value::String(s) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

fn parse_f64(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn parse_block(raw: &Value) -> Option<ActiveBlock> {
    let obj = raw.as_object()?;

    let start = obj.get("startTime").and_then(Value::as_str).unwrap_or("").to_string();
    let end   = obj.get("endTime").and_then(Value::as_str).unwrap_or("").to_string();

    let total  = parse_u64(obj.get("totalTokens").unwrap_or(&Value::Null));
    let input  = parse_u64(obj.get("inputTokens").unwrap_or(&Value::Null));
    let output = parse_u64(obj.get("outputTokens").unwrap_or(&Value::Null));
    let cr     = parse_u64(obj.get("cacheReadTokens").unwrap_or(&Value::Null));
    let cw     = parse_u64(obj.get("cacheWriteTokens").unwrap_or(&Value::Null));
    let cost   = parse_f64(obj.get("costUSD").unwrap_or(&Value::Null));

    // Derive usage percent from tokenLimitStatus
    let (usage_percent, token_limit) = {
        let tls = obj.get("tokenLimitStatus").and_then(Value::as_object);
        let limit = tls
            .and_then(|t| t.get("limit"))
            .map(parse_u64)
            .filter(|&l| l > 0);
        let pct = tls
            .and_then(|t| t.get("percentUsed"))
            .map(parse_f64)
            .map(|p| if p > 0.0 && p <= 1.0 { p * 100.0 } else { p });
        (pct, limit)
    };

    let models: Vec<String> = obj
        .get("models")
        .and_then(Value::as_object)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    Some(ActiveBlock {
        start_time: start,
        end_time: end,
        total_tokens: total,
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cr,
        cache_write_tokens: cw,
        cost_usd: cost,
        usage_percent,
        token_limit,
        models,
    })
}

fn parse_daily(raw: &Value) -> Option<DailyEntry> {
    let obj = raw.as_object()?;
    let period = obj.get("period").and_then(Value::as_str)?.to_string();

    let total  = parse_u64(obj.get("totalTokens").unwrap_or(&Value::Null));
    let input  = parse_u64(obj.get("inputTokens").unwrap_or(&Value::Null));
    let output = parse_u64(obj.get("outputTokens").unwrap_or(&Value::Null));
    let cr     = parse_u64(obj.get("cacheReadTokens").unwrap_or(&Value::Null));
    let cw     = parse_u64(obj.get("cacheWriteTokens").unwrap_or(&Value::Null));
    let cost   = parse_f64(obj.get("costUSD").unwrap_or(&Value::Null));

    let models: HashMap<String, ModelUsage> = obj
        .get("models")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| {
                    let mo = v.as_object()?;
                    Some((
                        k.clone(),
                        ModelUsage {
                            total_tokens: parse_u64(mo.get("tokens").unwrap_or(&Value::Null)),
                            cost_usd:     parse_f64(mo.get("costUSD").unwrap_or(&Value::Null)),
                        },
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    Some(DailyEntry { period, total_tokens: total, input_tokens: input, output_tokens: output, cache_read_tokens: cr, cache_write_tokens: cw, cost_usd: cost, models })
}

pub fn fetch(history_days: u32) -> Result<CcusageSnapshot, String> {
    if !is_available() {
        return Err("ccusage not found — run: npm install -g ccusage".into());
    }

    // Active block
    let active_block = run_ccusage(&["blocks", "--json", "--offline", "--active"])
        .and_then(|v| {
            v.get("blocks")
                .and_then(Value::as_array)
                .and_then(|arr| {
                    arr.iter()
                        .filter(|b| {
                            b.get("isGap").and_then(Value::as_bool) != Some(true)
                        })
                        .find(|b| b.get("isActive").and_then(Value::as_bool) == Some(true))
                        .and_then(|b| parse_block(b))
                })
        });

    // Daily history (covers today too)
    let since = since_date(history_days);
    let today  = today_str();
    let daily_list: Vec<DailyEntry> = run_ccusage(&["daily", "--json", "--offline", "--since", &since])
        .and_then(|v| v.get("daily").and_then(Value::as_array).cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|v| parse_daily(v))
        .collect();

    let today_entry = daily_list.iter().find(|d| d.period == today).cloned();

    Ok(CcusageSnapshot {
        active_block,
        today: today_entry,
        history: daily_list,
    })
}
