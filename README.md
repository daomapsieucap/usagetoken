# UsageToken

Simple Windows tray monitor for Claude token usage.

---

## Prerequisites

| Tool | Install |
|---|---|
| Rust (stable) | `winget install Rustlang.Rustup` then `rustup default stable` |
| Node.js 18+ | `winget install OpenJS.NodeJS` |
| Tauri CLI | `cargo install tauri-cli --version "^2"` |
| WebView2 | Included in Windows 10 21H2+ and Windows 11 |

ccusage is **bundled inside the app** — users do not need to install it.

---

## Screenshots

<div>
<img width="480" height="640" alt="Screenshot-1782701860" src="https://github.com/user-attachments/assets/118f0fb4-fcd9-43ea-9dce-3b489959f108" />
<br /> Full widget
</div>

<div>
<img width="240" height="155" alt="Screenshot-1782701854" src="https://github.com/user-attachments/assets/39efaba3-ca06-48ec-93df-b24ee3720cc8" />
<br /> Mini widget
</div>

---
## Dev

```powershell
pnpm install
pnpm build:sidecar   # compile the bundled ccusage binary (first time, or after updating ccusage)
cargo tauri dev
```

The popup window opens on left-click of the tray icon. Right-click shows: Open / Toggle widget / Quit.

---

## Release build (Windows)

```powershell
pnpm install
pnpm build:sidecar   # compile the bundled ccusage binary
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

**Secondary - local ccusage (bundled):**
Invokes a bundled ccusage binary (`ccusage daily --json --offline --since <date>`) to get per-day
token totals from local Claude Code logs. No separate ccusage installation is required.
Never runs an unbounded full-history scan.

**Cost figures** are estimates at public API rates. They are explicitly labeled in the UI and do NOT reflect what Anthropic charges Pro/Max subscribers.

---

## Auto-start on login

Toggle "Launch at login" in Settings. This uses `tauri-plugin-autostart` which writes the
appropriate registry key on Windows.
