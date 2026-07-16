import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Settings } from "../types";

const DEFAULT: Settings = {
  show_widget:            false,
  default_history_range:  30,
  launch_at_login:        false,
  debounce_ms:            750,
  server_poll_interval_s: 60,
  taskbar_overlay_enabled:       false,
  overlay_all_monitors_fallback: false,
  overlay_primary_only:          true,
  overlay_offset_x_overrides:    {},
};

export default function SettingsPanel() {
  const [settings, setSettings] = useState<Settings>(DEFAULT);
  const [saved, setSaved]       = useState(true);
  const [error, setError]       = useState<string | null>(null);

  useEffect(() => {
    invoke<Settings>("get_settings").then(s => { setSettings(s); setSaved(true); }).catch(console.error);
  }, []);

  const save = (next: Settings) => {
    setSettings(next);
    setSaved(false);
    invoke("save_settings", { settings: next })
      .then(() => { setSaved(true); setError(null); })
      .catch(e => setError(String(e)));
  };

  const toggle = (key: keyof Settings) =>
    save({ ...settings, [key]: !settings[key as keyof Settings] });

  const select = (key: keyof Settings, value: number) =>
    save({ ...settings, [key]: value });

  return (
    <div className="scroll-area" style={{ height: "100%" }}>
      <div className="card-title" style={{ color: "var(--fg2)", marginBottom: 12 }}>// settings</div>

      {error && <div className="banner banner-error">{error}</div>}

      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        <Row label="Show always-on widget">
          <Toggle checked={settings.show_widget} onChange={() => toggle("show_widget")} />
        </Row>

        <Row label="Launch at login">
          <Toggle checked={settings.launch_at_login} onChange={() => toggle("launch_at_login")} />
        </Row>

        <Row label="Taskbar overlay (per-monitor pill)">
          <Toggle checked={settings.taskbar_overlay_enabled} onChange={() => toggle("taskbar_overlay_enabled")} />
        </Row>

        {/* overlay_primary_only / overlay_all_monitors_fallback toggles hidden until multi-monitor is stable */}

        <Row label="Default history range">
          <select
            value={settings.default_history_range}
            onChange={e => select("default_history_range", parseInt(e.target.value))}
            style={{
              fontFamily: "var(--mono)", fontSize: 11,
              background: "var(--bg2)", border: "1px solid var(--border)",
              borderRadius: 4, padding: "3px 6px", color: "var(--fg)",
            }}
          >
            <option value={7}>7 days</option>
            <option value={30}>30 days</option>
            <option value={90}>90 days</option>
          </select>
        </Row>

        <Row label="File-change debounce (ms)">
          <input
            type="number"
            value={settings.debounce_ms}
            min={200} max={5000} step={50}
            onChange={e => select("debounce_ms", parseInt(e.target.value) || 750)}
            onBlur={() => save(settings)}
            style={{
              width: 72, fontFamily: "var(--mono)", fontSize: 11,
              background: "var(--bg2)", border: "1px solid var(--border)",
              borderRadius: 4, padding: "3px 6px", color: "var(--fg)",
            }}
          />
        </Row>

        <Row label="Server poll interval (s)">
          <input
            type="number"
            value={settings.server_poll_interval_s}
            min={30} max={3600} step={10}
            onChange={e => select("server_poll_interval_s", parseInt(e.target.value) || 60)}
            onBlur={() => save(settings)}
            style={{
              width: 72, fontFamily: "var(--mono)", fontSize: 11,
              background: "var(--bg2)", border: "1px solid var(--border)",
              borderRadius: 4, padding: "3px 6px", color: "var(--fg)",
            }}
          />
        </Row>
      </div>

      <div style={{ marginTop: 14, display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ fontSize: 10, color: saved ? "var(--green)" : "var(--fg2)" }}>
          {saved ? "all settings saved" : "saving…"}
        </span>
      </div>

    </div>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
      <span style={{ fontSize: 11, color: "var(--fg)" }}>{label}</span>
      {children}
    </div>
  );
}

function Toggle({ checked, onChange }: { checked: boolean; onChange: () => void }) {
  return (
    <button
      onClick={onChange}
      style={{
        width: 36, height: 20,
        borderRadius: 10,
        border: "none",
        background: checked ? "var(--blue)" : "var(--bg3)",
        cursor: "pointer",
        position: "relative",
        transition: "background 0.15s",
        flexShrink: 0,
      }}
    >
      <span style={{
        position: "absolute",
        top: 2, left: checked ? 18 : 2,
        width: 16, height: 16,
        borderRadius: "50%",
        background: "white",
        transition: "left 0.15s",
        display: "block",
      }} />
    </button>
  );
}
