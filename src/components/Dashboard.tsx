import { invoke } from "@tauri-apps/api/core";
import type { AppState, UsageWindow } from "../types";
import { fmtTokens, fmtPct, fmtCost, fmtAgo, fmtCountdown, gaugeColor } from "../types";

interface Props { state: AppState }

function RingGauge({ pct, color, size = 90 }: { pct?: number; color: string; size?: number }) {
  const r   = (size - 12) / 2;
  const circ = 2 * Math.PI * r;
  const fill  = pct != null ? Math.max(0, Math.min(1, pct / 100)) : 0;
  const dash  = fill * circ;
  const cx = size / 2, cy = size / 2;
  const label = pct != null ? fmtPct(pct) : "—";

  return (
    <div className="ring-gauge" style={{ width: size, height: size }}>
      <svg width={size} height={size}>
        {/* Track */}
        <circle cx={cx} cy={cy} r={r} fill="none" stroke="var(--bg3)" strokeWidth={8} />
        {/* Fill — start from top (rotate -90°) */}
        {pct != null && pct > 0 && (
          <circle
            cx={cx} cy={cy} r={r}
            fill="none"
            stroke={color}
            strokeWidth={8}
            strokeDasharray={`${dash} ${circ}`}
            strokeLinecap="round"
            transform={`rotate(-90 ${cx} ${cy})`}
          />
        )}
      </svg>
      <div className="ring-label" style={{ color: pct != null && pct > 0 ? color : "var(--fg2)", fontSize: size < 80 ? 11 : 13 }}>
        {label}
      </div>
    </div>
  );
}

function WindowRow({ win }: { win: UsageWindow }) {
  const color   = gaugeColor(win.percent_remaining);
  const pctUsed = 100 - win.percent_remaining;
  const is5h    = win.name === "5h";
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 4 }}>
      <span style={{
        fontSize: 9, fontWeight: "bold", padding: "1px 5px",
        background: is5h ? "var(--blue)" : "var(--fg2)",
        color: "white", borderRadius: 3,
      }}>
        {win.name}
      </span>
      <span style={{ fontSize: 12, fontWeight: is5h ? "bold" : "normal", color, minWidth: 60, textAlign: "right" }}>
        {fmtPct(pctUsed)} used
      </span>
      <span style={{ fontSize: 10, color: "var(--fg2)" }}>
        {win.reset_ts ? fmtCountdown(win.reset_ts) : ""}
      </span>
    </div>
  );
}

function ProgressBar({ pct, color }: { pct?: number; color: string }) {
  return (
    <div className="progress-bar">
      <div className="progress-fill" style={{ width: `${Math.max(0, Math.min(100, pct ?? 0))}%`, background: color }} />
    </div>
  );
}

export default function Dashboard({ state }: Props) {
  const { server, ccusage, error, refreshed_at } = state;

  const doRefresh = () => invoke("trigger_refresh").catch(console.error);

  // Primary window for server data
  const primaryWin = server?.windows.find(w => {
    const nameMap: Record<string, string> = { five_hour: "5h", seven_day: "7d" };
    return w.name === (nameMap[server.representative] ?? "5h");
  }) ?? server?.windows[0];

  const today = ccusage?.today;
  const block = ccusage?.active_block;

  return (
    <div className="scroll-area" style={{ height: "100%" }}>

      {error && <div className="banner banner-error">{error}</div>}

      {/* ── Server capacity card ─────────────────────────────────────────── */}
      <div className="card">
        <div className="card-inner">
          <div className="card-accent" style={{ background: "var(--blue)" }} />
          <div className="card-body">
            <div className="card-title" style={{ color: "var(--blue)" }}>
              // server capacity · claude.ai + Claude Code + Desktop
            </div>

            {server?.error && !server.windows.length ? (
              <div className="banner banner-warn" style={{ marginBottom: 0 }}>{server.error}</div>
            ) : (
              <div style={{ display: "flex", gap: 14, alignItems: "flex-start" }}>
                <RingGauge
                  pct={primaryWin != null ? 100 - primaryWin.percent_remaining : undefined}
                  color={gaugeColor(primaryWin?.percent_remaining ?? 100)}
                  size={90}
                />
                <div style={{ flex: 1, minWidth: 0 }}>
                  {server?.windows.map(w => <WindowRow key={w.name} win={w} />)}
                  {server?.error && <div className="disclaimer" style={{ marginTop: 4 }}>{server.error}</div>}
                </div>
              </div>
            )}

            {primaryWin && (
              <ProgressBar pct={100 - primaryWin.percent_remaining} color={gaugeColor(primaryWin.percent_remaining)} />
            )}
          </div>
        </div>
      </div>

      {/* Divider */}
      <div className="section-label">
        // usage details · <span className="disclaimer">estimated at API rates — not your subscription cost</span>
      </div>

      {/* ── Active block card ────────────────────────────────────────────── */}
      <div className="card">
        <div className="card-inner">
          <div className="card-accent" style={{ background: "var(--acc2)" }} />
          <div className="card-body">
            <div className="card-title" style={{ color: "var(--acc2)" }}>
              // current 5-hour block (local estimate)
            </div>
            {block ? (
              <>
                <div style={{ display: "flex", gap: 12, alignItems: "flex-start" }}>
                  <RingGauge
                    pct={block.usage_percent}
                    color={block.usage_percent != null ? gaugeColor(100 - block.usage_percent) : "var(--fg2)"}
                    size={76}
                  />
                  <div style={{ flex: 1 }}>
                    <div style={{ fontSize: 13, fontWeight: "bold", marginBottom: 3 }}>
                      {fmtTokens(block.total_tokens)} tokens
                    </div>
                    {block.token_limit && (
                      <div style={{ fontSize: 10, color: "var(--fg2)" }}>
                        limit ≈ {fmtTokens(block.token_limit)}
                      </div>
                    )}
                    <div style={{ fontSize: 10, color: "var(--fg2)", marginTop: 2 }}>
                      est. {fmtCost(block.cost_usd)} at API rates
                    </div>
                  </div>
                </div>
                {block.usage_percent != null && (
                  <ProgressBar
                    pct={block.usage_percent}
                    color={block.usage_percent >= 90 ? "var(--red)" : block.usage_percent >= 70 ? "var(--orange)" : "var(--acc2)"}
                  />
                )}
                <div style={{ marginTop: 8 }}>
                  <TokenBreakdown
                    input={block.input_tokens}
                    output={block.output_tokens}
                    cr={block.cache_read_tokens}
                    cw={block.cache_write_tokens}
                  />
                </div>
              </>
            ) : (
              <div style={{ color: "var(--fg2)", fontSize: 11 }}>No active block detected</div>
            )}
          </div>
        </div>
      </div>

      {/* ── Today card ──────────────────────────────────────────────────── */}
      <div className="card">
        <div className="card-inner">
          <div className="card-accent" style={{ background: "var(--fg2)" }} />
          <div className="card-body">
            <div className="card-title" style={{ color: "var(--fg2)" }}>// today</div>
            {today ? (
              <>
                <div style={{ fontSize: 15, fontWeight: "bold", marginBottom: 6 }}>
                  {fmtTokens(today.total_tokens)} tokens
                </div>
                <TokenBreakdown
                  input={today.input_tokens}
                  output={today.output_tokens}
                  cr={today.cache_read_tokens}
                  cw={today.cache_write_tokens}
                />
                <div style={{ fontSize: 10, color: "var(--fg2)", marginTop: 4 }}>
                  est. {fmtCost(today.cost_usd)} at API rates
                </div>
              </>
            ) : (
              <div style={{ color: "var(--fg2)", fontSize: 11 }}>No usage today</div>
            )}
          </div>
        </div>
      </div>

      {/* ── Bottom bar ──────────────────────────────────────────────────── */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", paddingTop: 4 }}>
        <button
          onClick={doRefresh}
          style={{
            background: "var(--bg3)", border: "none", borderRadius: 4,
            padding: "4px 10px", fontFamily: "var(--mono)", fontSize: 11,
            color: "var(--fg)", cursor: "pointer",
          }}
        >
          $ refresh
        </button>
        <span style={{ fontSize: 10, color: "var(--fg2)" }}>{fmtAgo(refreshed_at)}</span>
      </div>
    </div>
  );
}

function TokenBreakdown({ input, output, cr, cw }: { input: number; output: number; cr: number; cw: number }) {
  const rows: [string, number][] = [
    ["input",        input],
    ["output",       output],
    ["cache read",   cr],
    ["cache write",  cw],
  ];
  return (
    <div>
      {rows.map(([label, val]) => (
        <div key={label} className="token-row">
          <span className="token-label">{label}</span>
          <span className="token-value" style={{ fontSize: 11 }}>{fmtTokens(val)}</span>
        </div>
      ))}
    </div>
  );
}
