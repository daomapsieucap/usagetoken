# claudemeter

Windows system-tray app that shows your real remaining Claude Pro capacity plus local token consumption details.

---

## How it works

**Primary gauge (server-side):** Makes a minimal inference call every 60 seconds and reads the `anthropic-ratelimit-unified-*` response headers. These headers report 5-hour and 7-day rolling-window utilization for your account, covering usage across claude.ai, Claude Code, and Claude Desktop. The tray icon arc and dashboard headline reflect this number.

**Secondary details (local):** Reads token consumption from `~/.claude/projects/` via `ccusage` (offline, no API key). Shows estimated token counts and API-rate cost for the active 5-hour block, today, and the last 7 days. These are labeled clearly as estimates, not actual Pro charges.

If the OAuth token is missing or the network is down, the app falls back to the local ccusage estimate as the headline and shows a reason.

---

## Install dependencies

```
pip install pystray Pillow watchdog
```

Node.js (18+) and npm are also required for ccusage.
If `ccusage` is not already installed, the app installs it on first launch via `npm install -g ccusage`.

---

## OAuth token

The server-side gauge needs the OAuth token that Claude Code stores locally. It is read automatically in this order:

1. Environment variable `CLAUDE_CODE_OAUTH_TOKEN`
2. `~/.claude/.credentials.json` under `.claudeAiOauth.accessToken`

The token needs the `user:inference` scope. If the gauge shows a 401 error, run `claude setup-token` or type `/login` inside Claude Code to mint a fresh token.

The token is kept in memory only and is never logged or written anywhere.

---

## Run

```
pythonw claudemeter.py
```

`pythonw` launches without a console window. The tray icon appears in the Windows notification area.

- **Left-click or double-click** the icon to open the dashboard
- **Right-click** for "Open Dashboard" and "Quit"

The dashboard renders cached data instantly, then refreshes in the background. File changes in `~/.claude/projects/` trigger an immediate update (debounced 2-3 s). A 60-second timer also polls for server-side data while the tray is active.

The dashboard uses the bundled JetBrains Mono font from `assets/fonts/`. No system-wide font install needed.

---

## Auto-start on login

1. Press **Win + R**, type `shell:startup`, press Enter.
2. Create a shortcut to `pythonw.exe` in that folder.
3. Set the shortcut **Target** to:
   ```
   C:\Path\To\pythonw.exe  C:\Path\To\claudemeter.py
   ```
4. Set **Start in** to the folder containing `claudemeter.py`.

---

## Notes

- The server-side gauge is unofficial. Anthropic does not publish a dedicated usage REST endpoint for Pro subscribers. The data comes from `anthropic-ratelimit-unified-*` headers on inference responses. If Anthropic changes the header format, update `POLL_ENDPOINT` and `_parse_headers()` in `usage_provider.py`.
- Cost figures in the Usage Details section reflect public API pricing applied to your token counts. They are not what Anthropic bills you as a Pro subscriber.
- Do not set `ANTHROPIC_API_KEY` in your environment. If that variable is set, Claude Code bills your API account instead of your Pro subscription.
