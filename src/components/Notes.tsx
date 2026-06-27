import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export default function Notes() {
  const [text, setText]   = useState("");
  const [saved, setSaved] = useState(true);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    invoke<string>("load_notes").then(t => { setText(t); setSaved(true); }).catch(console.error);
  }, []);

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setText(e.target.value);
    setSaved(false);
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      invoke("save_notes", { text: e.target.value })
        .then(() => setSaved(true))
        .catch(console.error);
    }, 600);
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", padding: 12 }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 8 }}>
        <span className="card-title" style={{ color: "var(--fg2)" }}>// notes</span>
        <span style={{ fontSize: 10, color: saved ? "var(--green)" : "var(--fg2)" }}>
          {saved ? "saved" : "saving…"}
        </span>
      </div>
      <textarea
        value={text}
        onChange={handleChange}
        placeholder="Free-form notes. Auto-saved."
        style={{
          flex: 1,
          background: "var(--bg2)",
          border: "1px solid var(--border)",
          borderRadius: "var(--radius)",
          padding: "10px 12px",
          fontFamily: "var(--mono)",
          fontSize: 12,
          color: "var(--fg)",
          resize: "none",
          outline: "none",
          lineHeight: 1.6,
          WebkitAppRegion: "no-drag",
        } as React.CSSProperties}
      />
    </div>
  );
}
