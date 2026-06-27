import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppState } from "./types";
import Dashboard from "./components/Dashboard";
import History   from "./components/History";
import Notes     from "./components/Notes";
import SettingsPanel from "./components/SettingsPanel";

type Tab = "dashboard" | "history" | "notes" | "settings";

export default function App() {
  const [tab, setTab]       = useState<Tab>("dashboard");
  const [state, setState]   = useState<AppState>({});

  // Initial load
  useEffect(() => {
    invoke<AppState>("get_usage_data").then(setState).catch(console.error);
  }, []);

  // Push updates from Rust
  useEffect(() => {
    const unlisten = listen<AppState>("usage-updated", (e) => setState(e.payload));
    return () => { unlisten.then(f => f()); };
  }, []);

  const closePopup = () => getCurrentWindow().hide();

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh", background: "var(--bg)" }}>
      {/* Drag-region chrome */}
      <div className="win-chrome" data-tauri-drag-region>
        <span className="win-title" data-tauri-drag-region>
          dao@chau:~$ <span style={{ color: "var(--acc2)" }}>usage-tray --watch</span>
        </span>
        <button className="win-close" data-no-drag onClick={closePopup}>×</button>
      </div>

      {/* Tab bar */}
      <div className="tabs">
        {(["dashboard", "history", "notes", "settings"] as Tab[]).map(t => (
          <button key={t} className={`tab ${tab === t ? "active" : ""}`} onClick={() => setTab(t)}>
            {t}
          </button>
        ))}
      </div>

      {/* Content */}
      <div style={{ flex: 1, overflow: "hidden" }}>
        {tab === "dashboard" && <Dashboard state={state} />}
        {tab === "history"   && <History   state={state} />}
        {tab === "notes"     && <Notes />}
        {tab === "settings"  && <SettingsPanel />}
      </div>
    </div>
  );
}
