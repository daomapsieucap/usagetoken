export interface UsageWindow {
  name: string;
  utilization: number;
  percent_remaining: number;
  reset_ts: number;
  status: string;
}

export interface ServerSnapshot {
  windows: UsageWindow[];
  representative: string;
  overall_status: string;
  fetched_at: number;
  error?: string;
}

export interface ModelUsage {
  total_tokens: number;
  cost_usd: number;
}

export interface DailyEntry {
  period: string;
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  cost_usd: number;
  models: Record<string, ModelUsage>;
}

export interface CcusageSnapshot {
  history: DailyEntry[];
}

export interface UserInfo {
  subscription_type?: string;
  rate_limit_tier?: string;
}

export interface AppState {
  ccusage?: CcusageSnapshot;
  server?: ServerSnapshot;
  user_info?: UserInfo;
  error?: string;
  refreshed_at?: number;
}

export interface Settings {
  show_widget: boolean;
  default_history_range: number;
  launch_at_login: boolean;
  debounce_ms: number;
  server_poll_interval_s: number;
  taskbar_overlay_enabled: boolean;
  overlay_all_monitors_fallback: boolean;
  overlay_primary_only: boolean;
  overlay_offset_x_overrides: Record<string, number>;
}

// ── Format helpers ─────────────────────────────────────────────────────────────

export function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000)     return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

export function fmtPct(n: number): string {
  return `${Math.round(n)}%`;
}

export function fmtCost(n: number): string {
  if (n >= 1) return `$${n.toFixed(2)}`;
  return `$${n.toFixed(4)}`;
}

export function fmtAgo(ts?: number): string {
  if (!ts) return "";
  const s = Math.floor(Date.now() / 1000 - ts);
  if (s < 60)  return `updated ${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60)  return `updated ${m}m ago`;
  return `updated ${Math.floor(m / 60)}h ${m % 60}m ago`;
}

export function fmtCountdown(resetTs: number): string {
  const secsLeft = resetTs - Math.floor(Date.now() / 1000);
  if (secsLeft <= 0) return "resetting now";
  const m = Math.floor(secsLeft / 60);
  const h = Math.floor(m / 60);
  const d = Math.floor(h / 24);
  const hh = h % 24;
  const mm = m % 60;
  if (d > 0) return `resets in ${d}d ${hh}h ${String(mm).padStart(2, "0")}m`;
  if (h > 0) return `resets in ${h}h ${String(mm).padStart(2, "0")}m`;
  return `resets in ${m}m`;
}

export function gaugeColor(pctRemaining: number): string {
  if (pctRemaining >= 50) return "var(--green)";
  if (pctRemaining >= 20) return "var(--orange)";
  return "var(--red)";
}
