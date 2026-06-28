# UsageToken

Simple Windows tray monitor for Claude token usage.

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
pnpm install
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
Runs `ccusage daily --json --offline --since <date>` to get per-day token totals from local logs.
Never runs an unbounded full-history scan.

**Cost figures** are estimates at public API rates. They are explicitly labeled in the UI and do NOT reflect what Anthropic charges Pro/Max subscribers.

---

## Auto-start on login

Toggle "Launch at login" in Settings. This uses `tauri-plugin-autostart` which writes the
appropriate registry key on Windows.
