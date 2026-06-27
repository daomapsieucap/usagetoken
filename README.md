# UsageToken

Windows system-tray app for Claude Code token usage. Tauri 2 + React + pure Rust backend.

---

## Prerequisites

| Tool | Install |
|---|---|
| Rust (stable) | `winget install Rustlang.Rustup` then `rustup default stable` |
| Node.js 18+ | `winget install OpenJS.NodeJS` |
| Tauri CLI | `cargo install tauri-cli --version "^2"` |
| ccusage | `npm install -g ccusage` |
| WebView2 | Included in Windows 10 21H2+ and Windows 11 |

---

## Dev

```powershell
npm install
cargo tauri dev
```

The popup window opens on left-click of the tray icon. Right-click shows: Open / Toggle widget / Quit.

---

## Release build (Windows)

```powershell
cargo tauri build
```

Produces `src-tauri/target/release/usage-token.exe` and an NSIS installer under `src-tauri/target/release/bundle/nsis/`.

---

## App icon

Placeholder icons are in `src-tauri/icons/`. Replace them with your own:

- `32x32.png` - used as the system-tray icon
- `128x128.png` - used in the installer
- `icon.ico` - Windows taskbar/exe icon (should contain 16×16, 24×24, 32×32, 48×48, 256×256)

The tray icon is loaded in `src-tauri/src/tray.rs` via `app.default_window_icon()`.
To set a separate tray icon at runtime call `tray.set_icon(Some(Image::from_path(...)?))`.

---

## Data sources

**Primary - server-side rate-limit headers:**
Makes a minimal 1-token inference call to `api.anthropic.com/v1/messages` and reads the
`anthropic-ratelimit-unified-*` response headers. These reflect shared usage across claude.ai,
Claude Code, and Claude Desktop.

Credentials are read (in order) from:
1. `CLAUDE_CODE_OAUTH_TOKEN` environment variable
2. `~/.claude/.credentials.json` → `.claudeAiOauth.accessToken`

Each poll uses ≈9 Pro quota tokens - well under 0.3 % of a 5-hour window.

**Secondary - local ccusage:**
Shells out to `ccusage` with `--offline --since <date>` to parse `~/.claude/projects/` JSONL logs.
Never runs an unbounded full-history scan.

**Cost figures** are estimates at public API rates. They are explicitly labeled in the UI and can be hidden
in Settings. They do NOT reflect what Anthropic charges Pro/Max subscribers.

---

## Architecture

```
Rust backend
 ├── lib.rs          - Tauri builder, window/tray setup, file watcher, poll loop
 ├── tray.rs         - TrayIcon builder, left/right click, blur-hide logic
 ├── manager.rs      - refresh() - spawns ccusage + server_poll, emits "usage-updated"
 ├── ccusage.rs      - runs `ccusage blocks/daily --offline --since`
 ├── server_poll.rs  - HTTPS call to Anthropic, parses rate-limit headers
 ├── watcher.rs      - notify-based file watcher with debounce
 ├── commands.rs     - Tauri commands: get_usage_data, trigger_refresh, settings r/w
 └── data.rs         - typed structs + APP_NAME constant

React frontend (src/)
 ├── App.tsx                  - popup window: tab bar + tab routing
 ├── widget.tsx               - widget entry point
 └── components/
     ├── Dashboard.tsx        - server capacity + active block + today
     ├── History.tsx          - daily bar chart (Recharts) + table
     ├── SettingsPanel.tsx    - all settings with immediate save
     ├── Widget.tsx           - always-on floating mini-view
     └── Sparkline.tsx        - tiny SVG sparkline
```

Two windows:
- `popup` - 480×640, borderless, skip-taskbar, always-on-top, hidden at start.
  Shown on left-click, hidden on blur or repeat click. Positioned above the tray icon.
  Three tabs: dashboard / history / settings.
- `widget` - 220×130, borderless, skip-taskbar, always-on-top, draggable.
  Toggled from the tray right-click menu or Settings.

---

## Auto-start on login

Toggle "Launch at login" in Settings. This uses `tauri-plugin-autostart` which writes the
appropriate registry key on Windows.
