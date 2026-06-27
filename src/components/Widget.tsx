import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppState } from "../types";
import { fmtTokens, fmtPct, gaugeColor } from "../types";
import Sparkline from "./Sparkline";

export default function WidgetApp() {
  const [state, setState] = useState<AppState>({});

  useEffect(() => {
    invoke<AppState>("get_usage_data").then(setState).catch(console.error);
    const unsub = listen<AppState>("usage-updated", e => setState(e.payload));
    return () => { unsub.then(f => f()); };
  }, []);

  const today     = state.ccusage?.today;
  const primary   = state.server?.windows[0];
  const sparkData = (state.ccusage?.history ?? []).slice(-7).map(d => d.total_tokens);

  return (
    <div
      data-tauri-drag-region
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        height: "100vh",
        padding: "0 12px",
        background: "rgba(243,243,243,0.96)",
        backdropFilter: "blur(8px)",
        cursor: "move",
        position: "relative",
      }}
    >
      <button
        data-no-drag
        onClick={() => getCurrentWindow().hide()}
        style={{
          position: "absolute", top: 4, right: 6,
          border: "none", background: "none",
          fontSize: 14, color: "var(--fg2)", cursor: "pointer",
        }}
      >
        ×
      </button>

      {/* Mini ring */}
      {primary && (
        <MiniRing pct={primary.percent_remaining} />
      )}

      {/* Today headline */}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13, fontWeight: "bold", lineHeight: 1.2, fontFamily: "var(--mono)" }}>
          {today ? fmtTokens(today.total_tokens) : "—"}
        </div>
        <div style={{ fontSize: 9, color: "var(--fg2)", fontFamily: "var(--mono)" }}>today</div>
      </div>

      {/* Sparkline */}
      {sparkData.length > 1 && <Sparkline data={sparkData} />}
    </div>
  );
}

function MiniRing({ pct }: { pct: number }) {
  const r    = 16;
  const circ = 2 * Math.PI * r;
  const fill = (pct / 100) * circ;
  const color = gaugeColor(pct);

  return (
    <div style={{ position: "relative", width: 44, height: 44, flexShrink: 0 }}>
      <svg width={44} height={44}>
        <circle cx={22} cy={22} r={r} fill="none" stroke="var(--bg3)" strokeWidth={5} />
        {pct > 0 && (
          <circle
            cx={22} cy={22} r={r} fill="none"
            stroke={color} strokeWidth={5}
            strokeDasharray={`${fill} ${circ}`}
            strokeLinecap="round"
            transform="rotate(-90 22 22)"
          />
        )}
      </svg>
      <div style={{
        position: "absolute", inset: 0,
        display: "flex", alignItems: "center", justifyContent: "center",
        fontSize: 9, fontWeight: "bold", fontFamily: "var(--mono)", color,
      }}>
        {fmtPct(pct)}
      </div>
    </div>
  );
}
