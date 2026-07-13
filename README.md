# UsageToken

A Windows tray app that tracks your Claude usage in real time: 5h and 7d rate-limit
windows, daily token/cost history, and an always-on-top mini widget - all without
needing ccusage installed separately.

---

## Features

- **Tray popup dashboard** - left-click the tray icon for ring gauges on the 5h
  rolling and 7d weekly usage windows, each with a status pill, today's token/cost
  total, and a 7-day sparkline.
- **History tab** - stacked bar chart (input / output / cache tokens) plus a daily
  table for the last 7, 30, or 90 days, with running totals and estimated cost.
- **Always-on-top mini widget** - a compact, draggable panel with its own 5h/7d
  rings and countdowns. Mutually exclusive with the popup: opening one hides the
  other, and the tray context menu (Open / Toggle widget / Quit) controls both.
- **Dual data sources** - live server rate-limit headers for accurate 5h/7d
  windows, plus a bundled ccusage sidecar for local per-day token history. See
  [Data sources](#data-sources) below.
- **Settings panel** - toggle the mini widget, launch-at-login, default history
  range, file-watch debounce, and server poll interval, all persisted to disk.
- **Light/dark theme** that follows the OS.
- **Built for long sessions** - tuned to stay smooth across 8+ hour continuous
  runs, including sleep/wake cycles. See [Long-session stability](#long-session-stability).

---

## Prerequisites

| Tool | Install |
|---|---|
| Rust (stable) | `winget install Rustlang.Rustup` then `rustup default stable` |
| Node.js 18+ | `winget install OpenJS.NodeJS` |
| Tauri CLI | `cargo install tauri-cli --version "^2"` |
| WebView2 | Included in Windows 10 21H2+ and Windows 11 |

ccusage is **bundled inside the app** - users do not need to install it.

---

## Screenshots

<table>
  <tr>
    <td align="center" width="55%">
      <img width="100%" alt="Full popup dashboard with 5h and 7d usage gauges" src="https://github.com/user-attachments/assets/118f0fb4-fcd9-43ea-9dce-3b489959f108" />
      <br />
      <sub><b>Popup dashboard</b> - 5h / 7d gauges, status pills, sparkline</sub>
    </td>
    <td align="center" width="45%">
      <img width="100%" alt="Always-on-top mini widget showing 5h and 7d rings" src="https://github.com/user-attachments/assets/39efaba3-ca06-48ec-93df-b24ee3720cc8" />
      <br />
      <sub><b>Mini widget</b> - always-on-top, draggable</sub>
    </td>
  </tr>
</table>

---

## Dev

```powershell
pnpm install
pnpm build:sidecar   # stage the bundled ccusage binary (first time, or after updating ccusage)
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
2. `~/.claude/.credentials.json` -> `.claudeAiOauth.accessToken`

Each poll uses ~9 Pro quota tokens - well under 0.3% of a 5-hour window. Poll interval is
configurable in Settings (default 60s).

**Secondary - local ccusage (bundled):**
Invokes a bundled ccusage binary (`ccusage daily --json --offline --since <date>`) to get per-day
token totals from local Claude Code logs, plus a file watcher on `~/.claude/projects/` that
triggers a debounced refresh whenever session logs change. No separate ccusage installation is
required. Never runs an unbounded full-history scan.

**Cost figures** are estimates at public API rates. They are explicitly labeled in the UI and do NOT reflect what Anthropic charges Pro/Max subscribers.

---

## Auto-start on login

Toggle "Launch at login" in Settings. This uses `tauri-plugin-autostart` which writes the
appropriate registry key on Windows.

---

## Long-session stability

The app is designed to stay smooth across an 8+ hour continuous run: tray popup, mini widget
drag, charts, and polling all avoid per-event allocations, leaked listeners, and unbounded
growth. See [docs/soak-test.md](docs/soak-test.md) for the full checklist and how to enable the
optional memory logger (`UT_SOAK_LOG=1`) that samples process and JS heap memory every 5 minutes.
