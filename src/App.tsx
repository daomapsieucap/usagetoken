import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import type { AppState, Settings } from "./types";
import Dashboard from "./components/Dashboard";
import History   from "./components/History";
import SettingsPanel from "./components/SettingsPanel";
import { TickerProvider } from "./ticker";
import { useSoakLogger } from "./soak";

type Tab = "dashboard" | "history" | "settings";

export default function App() {
  const [tab, setTab]               = useState<Tab>("dashboard");
  const [state, setState]           = useState<AppState>({});
  const [widgetOn, setWidgetOn]     = useState(false);
  const [version, setVersion]       = useState<string | null>(null);

  // Initial load
  useEffect(() => {
    invoke<AppState>("get_usage_data").then(setState).catch(console.error);
    invoke<Settings>("get_settings").then(s => setWidgetOn(s.show_widget)).catch(console.error);
    getVersion().then(setVersion).catch(console.error);
  }, []);

  // Push updates from Rust
  useEffect(() => {
    const unlisten = listen<AppState>("usage-updated", (e) => setState(e.payload));
    return () => { unlisten.then(f => f()); };
  }, []);

  // Taskbar overlay's right-click menu can ask the popup to jump to settings.
  useEffect(() => {
    const unlisten = listen("open-settings", () => setTab("settings"));
    return () => { unlisten.then(f => f()); };
  }, []);

  useSoakLogger();

  const closePopup = () => getCurrentWindow().hide();

  async function toggleWidget() {
    const show = await invoke<boolean>("toggle_widget");
    setWidgetOn(show);
    if (show) getCurrentWindow().hide();
  }

  return (
    <TickerProvider>
    <div style={{ display: "flex", flexDirection: "column", height: "100vh", background: "var(--bg)" }}>
      {/* Drag-region chrome */}
      <div className="win-chrome" data-tauri-drag-region>
        <span className="win-title" data-tauri-drag-region>
          dao@chau:~$ <span style={{ color: "var(--acc2)" }}>usagetoken --watch</span>
          {version && (
            <span style={{ color: "var(--fg2)", marginLeft: 6, fontSize: "0.85em" }}>
              v{version}
            </span>
          )}
        </span>
        <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
          <button
            className="win-action"
            data-no-drag
            onClick={toggleWidget}
            title={widgetOn ? "Hide mini widget" : "Show mini widget"}
            style={{ color: widgetOn ? "var(--acc2)" : undefined }}
          >
            <svg width="12" height="9" viewBox="0 0 12 9" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round">
              <rect x="0.6" y="0.6" width="10.8" height="7.8" rx="1" />
              <rect x="6.5" y="4.5" width="4" height="3" rx="0.5" fill="currentColor" stroke="none" />
            </svg>
          </button>
          <button className="win-close" data-no-drag onClick={closePopup}>×</button>
        </div>
      </div>

      {/* Tab bar */}
      <div className="tabs">
        {(["dashboard", "history", "settings"] as Tab[]).map(t => (
          <button key={t} className={`tab ${tab === t ? "active" : ""}`} onClick={() => setTab(t)}>
            {t}
          </button>
        ))}
      </div>

      {/* Content */}
      <div style={{ flex: 1, overflow: "hidden" }}>
        {tab === "dashboard" && <Dashboard state={state} />}
        {tab === "history"   && <History   state={state} />}
        {tab === "settings"  && <SettingsPanel />}
      </div>
    </div>
    </TickerProvider>
  );
}
