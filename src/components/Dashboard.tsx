import { useRef, useLayoutEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppState, UsageWindow, CcusageSnapshot } from "../types";
import { fmtPct, fmtAgo, fmtCountdown, fmtTokens, fmtCost, gaugeColor } from "../types";

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
        <circle cx={cx} cy={cy} r={r} fill="none" stroke="var(--bg3)" strokeWidth={8} />
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

function MiniBarChart({ data, color }: { data: number[]; color: string }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [W, setW] = useState(0);

  useLayoutEffect(() => {
    if (containerRef.current) setW(containerRef.current.offsetWidth);
  }, []);

  const BAR_H = 28, TOP = 12, BOT = 11;
  const H = TOP + BAR_H + BOT;
  const n = data.length;
  const gap = 3;
  const barW = W > 0 ? Math.floor((W - (n - 1) * gap) / n) : 0;
  const max = Math.max(...data, 1);
  const maxIdx = data.indexOf(max);
  const todayIdx = n - 1;

  return (
    <div ref={containerRef} style={{ width: "100%" }}>
      {W > 0 && (
        <svg width={W} height={H} style={{ display: "block", overflow: "visible" }}>
          {data.map((v, i) => {
            const x = i * (barW + gap);
            const bh = Math.max(2, (v / max) * BAR_H);
            const y = TOP + BAR_H - bh;
            return (
              <rect
                key={i} x={x} y={y} width={barW} height={bh} rx={1}
                fill={color} opacity={i === todayIdx ? 1 : 0.35}
              />
            );
          })}
          <text
            x={maxIdx * (barW + gap) + barW / 2} y={TOP - 2}
            textAnchor="middle" fontSize={8} fontWeight="bold"
            fill={color} fontFamily="var(--mono)"
          >
            {fmtTokens(max)}
          </text>
          {todayIdx !== maxIdx && (
            <text
              x={todayIdx * (barW + gap) + barW / 2} y={TOP - 2}
              textAnchor="middle" fontSize={8}
              fill={color} fontFamily="var(--mono)"
            >
              {fmtTokens(data[todayIdx])}
            </text>
          )}
          <text x={0} y={H} textAnchor="start" fontSize={8} fill="var(--fg2)" fontFamily="var(--mono)">7d ago</text>
          <text x={W} y={H} textAnchor="end" fontSize={8} fill={color} fontFamily="var(--mono)">today</text>
        </svg>
      )}
    </div>
  );
}

function statusColor(status: string): string {
  if (status === "ok")       return "var(--green)";
  if (status === "warning")  return "var(--orange)";
  if (status === "critical") return "var(--red)";
  return "var(--fg2)";
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

function StatsRow({ status, tokens, cost }: { status: string; tokens?: number; cost?: number }) {
  const sc = statusColor(status);
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 4 }}>
      <span style={{ fontSize: 10, color: sc, fontWeight: "bold" }}>● {status}</span>
      {tokens != null && (
        <>
          <span style={{ fontSize: 10, color: "var(--fg2)" }}>·</span>
          <span style={{ fontSize: 10, color: "var(--fg2)" }}>{fmtTokens(tokens)} tok</span>
        </>
      )}
      {cost != null && (
        <>
          <span style={{ fontSize: 10, color: "var(--fg2)" }}>·</span>
          <span style={{ fontSize: 10, color: "var(--fg2)" }}>{fmtCost(cost)}</span>
        </>
      )}
    </div>
  );
}

function UsageCard({
  win,
  accentColor,
  title,
  error,
  noData,
  ccusage,
  showSparkline,
}: {
  win?: UsageWindow;
  accentColor: string;
  title: string;
  error?: string;
  noData?: boolean;
  ccusage?: CcusageSnapshot;
  showSparkline?: boolean;
}) {
  const pctUsed = win != null ? 100 - win.percent_remaining : undefined;

  const today = new Date().toISOString().slice(0, 10);
  const todayEntry = ccusage?.history.find(d => d.period === today);

  const sparkData = showSparkline && ccusage
    ? ccusage.history
        .slice()
        .sort((a, b) => a.period.localeCompare(b.period))
        .slice(-7)
        .map(d => d.total_tokens)
    : [];

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
                {win && (
                  <StatsRow
                    status={win.status}
                    tokens={todayEntry?.total_tokens}
                    cost={todayEntry?.cost_usd}
                  />
                )}
                {error && <div className="disclaimer" style={{ marginTop: 4 }}>{error}</div>}
                {showSparkline && sparkData.length >= 2 && (
                  <div style={{ marginTop: 6 }}>
                    <MiniBarChart data={sparkData} color={accentColor} />
                  </div>
                )}
              </div>
            </div>
          )}
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
        ccusage={state.ccusage}
      />

      {/* ── 7d weekly window card ────────────────────────────────────────── */}
      <UsageCard
        win={win7d}
        accentColor="var(--navy)"
        title="// 7d weekly · claude.ai + Claude Code + Desktop"
        error={noServerData ? server?.error : undefined}
        noData={noServerData}
        ccusage={state.ccusage}
        showSparkline
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
