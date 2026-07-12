import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppState, UsageWindow } from "../types";
import { fmtPct } from "../types";
import { TickerProvider } from "../ticker";
import { Countdown } from "./TimeDisplays";

export default function WidgetApp() {
  const [state, setState] = useState<AppState>({});

  useEffect(() => {
    invoke<AppState>("get_usage_data").then(setState).catch(console.error);
    const unsub = listen<AppState>("usage-updated", e => setState(e.payload));
    return () => { unsub.then(f => f()); };
  }, []);

  const win5h = state.server?.windows.find(w => w.name === "5h");
  const win7d = state.server?.windows.find(w => w.name === "7d");

  return (
    <TickerProvider>
    <div
      data-tauri-drag-region
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100vh",
        background: "var(--bg2)",
      }}
    >
      {/* Header */}
      <div style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "5px 10px 4px",
        borderBottom: "1px solid var(--border)",
        flexShrink: 0,
      }}>
        <span style={{
          fontSize: 9, fontWeight: "bold", color: "var(--fg2)",
          letterSpacing: "0.08em", fontFamily: "var(--mono)",
        }}>
          USAGETOKEN
        </span>
        <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
          <button
            className="win-action"
            data-no-drag
            onClick={() => invoke("show_popup")}
            title="Open full view"
          >
            <svg width="9" height="9" viewBox="0 0 9 9" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="1,3.5 1,1 3.5,1" />
              <polyline points="5.5,8 8,8 8,5.5" />
              <line x1="1.5" y1="1.5" x2="7.5" y2="7.5" />
            </svg>
          </button>
          <button
            className="win-close"
            data-no-drag
            onClick={() => getCurrentWindow().hide()}
          >
            ×
          </button>
        </div>
      </div>

      {/* Body: two mini panels side by side */}
      <div style={{
        display: "flex",
        flex: 1,
        padding: "6px 8px",
        gap: 6,
        background: "var(--bg)",
      }}>
        <MiniPanel win={win5h} accentColor="var(--acc2)" label="5h" />
        <div style={{ width: 1, background: "var(--border)", alignSelf: "stretch" }} />
        <MiniPanel win={win7d} accentColor="var(--navy)" label="7d" />
      </div>
    </div>
    </TickerProvider>
  );
}

function MiniRing({ pct, color, size }: { pct?: number; color: string; size: number }) {
  const r = (size - 7) / 2;
  const circ = 2 * Math.PI * r;
  const dash = pct != null ? Math.max(0, Math.min(1, pct / 100)) * circ : 0;
  const cx = size / 2, cy = size / 2;

  return (
    <div style={{ position: "relative", width: size, height: size, flexShrink: 0 }}>
      <svg width={size} height={size}>
        <circle cx={cx} cy={cy} r={r} fill="none" stroke="var(--bg3)" strokeWidth={5} />
        {pct != null && pct > 0 && (
          <circle
            cx={cx} cy={cy} r={r} fill="none"
            stroke={color} strokeWidth={5}
            strokeDasharray={`${dash} ${circ}`}
            strokeLinecap="round"
            transform={`translate(${size} 0) scale(-1 1) rotate(-90 ${cx} ${cy})`}
          />
        )}
      </svg>
      <div style={{
        position: "absolute", inset: 0,
        display: "flex", alignItems: "center", justifyContent: "center",
        fontSize: 11, fontWeight: "bold", fontFamily: "var(--mono)", color,
      }}>
        {pct != null ? fmtPct(pct) : "—"}
      </div>
    </div>
  );
}

function MiniPanel({ win, accentColor, label }: { win?: UsageWindow; accentColor: string; label: string }) {
  const pctUsed = win != null ? 100 - win.percent_remaining : undefined;

  return (
    <div style={{
      flex: 1,
      display: "flex",
      flexDirection: "column",
      alignItems: "center",
      justifyContent: "center",
      gap: 5,
    }}>
      <MiniRing pct={pctUsed} color={accentColor} size={66} />
      <div style={{ textAlign: "center", lineHeight: 1.4 }}>
        <div style={{ fontSize: 10, fontWeight: "bold", color: accentColor, fontFamily: "var(--mono)" }}>
          // {label}
        </div>
        {win?.reset_ts && (
          <div style={{ fontSize: 9, color: "var(--fg2)", fontFamily: "var(--mono)" }}>
            <Countdown resetTs={win.reset_ts} />
          </div>
        )}
      </div>
    </div>
  );
}
