# Claude Meter

Windows system-tray app that tracks your Claude Code token consumption in real time.

Reads data from `~/.claude/projects/` via **ccusage** — locally, offline, with no calls to the Anthropic API.

---

## Install dependencies

```
pip install pystray Pillow watchdog
```

Node.js (≥ 18) and npm are also required for ccusage.  
If `ccusage` is not already installed, the app installs it on first launch via `npm install -g ccusage`.

---

## Run

```
pythonw claudemeter.py
```

`pythonw` launches the script without a lingering console window.
The tray icon appears in the Windows notification area (bottom-right of the taskbar).

- **Left-click** or **double-click** the icon → opens the dashboard
- **Right-click** → menu with "Open Dashboard" and "Quit"

The dashboard renders cached data instantly, then refreshes in the background.
File changes in `~/.claude/projects/` trigger an update automatically (debounced 2-3 s).
A 60-second safety timer runs as a fallback when no file events arrive.

The dashboard uses the bundled JetBrains Mono font from `assets/fonts/`.
Users do not need to install the font system-wide.

---

## Auto-start on login

1. Press **Win + R** and type `shell:startup`, then press Enter.
2. Create a shortcut to `pythonw.exe` in that folder.
3. Set the shortcut's **Target** to:
   ```
   C:\Path\To\pythonw.exe  C:\Path\To\claudemeter.py
   ```
4. Set **Start in** to the folder containing `claudemeter.py`.

The app will launch silently on login, with no console window.

---

## Notes

- All data is local. No network calls are made by this app.
- The current block percent is derived from `ccusage blocks --token-limit max` when ccusage can infer a quota for the active block. If ccusage cannot provide a percent, Claude Meter still shows the block token count.
- Any dollar figures shown are labeled *"estimated API-rate cost (not my actual Pro charge)"* — they reflect the public API pricing applied to your token counts and are not what Anthropic bills you.

> ⚠️ **Do NOT set `ANTHROPIC_API_KEY`** in your environment. If that variable is set, Claude Code bills your API account instead of your Pro subscription.
