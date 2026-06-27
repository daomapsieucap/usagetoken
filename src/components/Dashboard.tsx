import { invoke } from "@tauri-apps/api/core";
import type { AppState, UsageWindow } from "../types";
import { fmtPct, fmtAgo, fmtCountdown, gaugeColor } from "../types";

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
        {/* Fill — start from left (9 o'clock), sweeps clockwise left→top→right */}
        {pct != null && pct > 0 && (
          <circle
            cx={cx} cy={cy} r={r}
            fill="none"
            stroke={color}
            strokeWidth={8}
            strokeDasharray={`${dash} ${circ}`}
            strokeLinecap="round"
            transform={`translate(${size} 0) scale(-1 1) rotate(-90 ${cx} ${cy})`}
          />
        )}
      </svg>
      <div className="ring-label" style={{ color: pct != null && pct > 0 ? color : "var(--fg2)", fontSize: size < 80 ? 11 : 13 }}>
        {label}
      </div>
    </div>
  );
}

function WindowRow({ win, accentColor }: { win: UsageWindow; accentColor?: string }) {
  const color   = gaugeColor(win.percent_remaining);
  const pctUsed = 100 - win.percent_remaining;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 4 }}>
      <span style={{
        fontSize: 9, fontWeight: "bold", padding: "1px 5px",
        background: accentColor ?? "var(--fg2)",
        color: "white", borderRadius: 3,
      }}>
        {win.name}
      </span>
      <span style={{ fontSize: 12, fontWeight: "bold", color, minWidth: 60, textAlign: "right" }}>
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

function UsageCard({
  win,
  accentColor,
  title,
  error,
  noData,
}: {
  win?: UsageWindow;
  accentColor: string;
  title: string;
  error?: string;
  noData?: boolean;
}) {
  const pctUsed = win != null ? 100 - win.percent_remaining : undefined;
  const color = gaugeColor(win?.percent_remaining ?? 100);
  return (
    <div className="card">
      <div className="card-inner">
        <div className="card-accent" style={{ background: accentColor }} />
        <div className="card-body">
          <div className="card-title" style={{ color: accentColor }}>{title}</div>
          {noData ? (
            <div className="banner banner-warn" style={{ marginBottom: 0 }}>{error}</div>
          ) : (
            <div style={{ display: "flex", gap: 14, alignItems: "flex-start" }}>
              <RingGauge pct={pctUsed} color={accentColor} size={90} />
              <div style={{ flex: 1, minWidth: 0 }}>
                {win && <WindowRow win={win} accentColor={accentColor} />}
                {error && <div className="disclaimer" style={{ marginTop: 4 }}>{error}</div>}
              </div>
            </div>
          )}
          {win && <ProgressBar pct={pctUsed} color={color} />}
        </div>
      </div>
    </div>
  );
}

export default function Dashboard({ state }: Props) {
  const { server, error, refreshed_at } = state;

  const doRefresh = () => invoke("trigger_refresh").catch(console.error);

  const win5h = server?.windows.find(w => w.name === "5h");
  const win7d = server?.windows.find(w => w.name === "7d");
  const noServerData = !!server?.error && !server.windows.length;

  return (
    <div className="scroll-area" style={{ height: "100%" }}>

      {error && <div className="banner banner-error">{error}</div>}

      {/* ── 5h rolling window card ───────────────────────────────────────── */}
      <UsageCard
        win={win5h}
        accentColor="var(--acc2)"
        title="// 5h rolling · claude.ai + Claude Code + Desktop"
        error={noServerData ? server?.error : undefined}
        noData={noServerData}
      />

      {/* ── 7d weekly window card ────────────────────────────────────────── */}
      <UsageCard
        win={win7d}
        accentColor="var(--green)"
        title="// 7d weekly · claude.ai + Claude Code + Desktop"
        error={noServerData ? server?.error : undefined}
        noData={noServerData}
      />

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

