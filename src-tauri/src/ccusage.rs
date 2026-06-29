use crate::data::{CcusageSnapshot, DailyEntry, ModelUsage};
use chrono::{Duration, Local};
use serde_json::Value;
use std::collections::HashMap;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

async fn run_sidecar(app: &AppHandle, args: &[&str]) -> Option<Value> {
    let output = app
        .shell()
        .sidecar("ccusage")
        .ok()?
        .args(args)
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let s = std::str::from_utf8(&output.stdout).ok()?;
        serde_json::from_str::<Value>(s.trim()).ok()
    } else {
        None
    }
}

fn run_ccusage(app: &AppHandle, args: &[&str]) -> Option<Value> {
    tauri::async_runtime::block_on(run_sidecar(app, args))
}

fn since_date(days: u32) -> String {
    let dt = Local::now() - Duration::days(days as i64);
    dt.format("%Y-%m-%d").to_string()
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

pub fn fetch(app: &AppHandle, history_days: u32) -> Result<CcusageSnapshot, String> {
    let since = since_date(history_days);

    let raw = run_ccusage(app, &["daily", "--json", "--offline", "--since", &since])
        .ok_or_else(|| "Bundled ccusage failed — try reinstalling UsageToken".to_string())?;

    let history = raw
        .get("daily")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(parse_daily).collect())
        .unwrap_or_default();

    Ok(CcusageSnapshot { history })
}
