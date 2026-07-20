# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

UsageToken is a Windows tray app (Tauri 2 + React/TypeScript frontend, Rust backend) that tracks
Claude usage in real time: 5h/7d rate-limit windows from the server, daily token/cost history from
a bundled `ccusage` sidecar binary, plus a mini widget and an optional native taskbar overlay.

## Commands

```powershell
pnpm install
pnpm build:sidecar    # stage the bundled ccusage native binary — required before first dev/build,
                       # and again after bumping the `ccusage` devDependency version
cargo tauri dev        # run the app in dev mode (spawns `vite` via beforeDevCommand)
cargo tauri build       # produce the release exe + NSIS installer
pnpm build              # frontend only: `tsc && vite build` (type-checks then bundles to dist/)
pnpm dev                # frontend-only dev server (vite), no Tauri shell — use `cargo tauri dev` normally
```

There is no JS test runner or linter configured (no eslint/vitest/jest in package.json) — `pnpm build`
(tsc) is the only frontend check. Rust changes are checked with `cargo check` / `cargo build` from
`src-tauri/`. There is no automated Rust test suite either; manual verification (soak testing) is the
project's own QA method — see `docs/soak-test.md`.

`pnpm build:sidecar` (`scripts/build-sidecar.mjs`) copies the real native ccusage binary (not the
`cli.js` dispatcher — that breaks in a bundled context) from ccusage's platform-specific optional
dependency package into `src-tauri/binaries/ccusage-{target-triple}.exe`, following Tauri's sidecar
naming convention. Re-run it whenever the `ccusage` version in `package.json` changes.

## Architecture

**Two-process split**: a Rust/Tauri backend owns all state, polling, and OS integration; the React
frontend is a thin, mostly stateless view over two windows (`popup`, `widget`) that both read the
same `AppState` snapshot pushed from Rust.

### Data flow (single source of truth: `AppState`)

- `src-tauri/src/manager.rs::refresh()` is the one place that mutates shared state. It spawns a
  thread that calls `ccusage::fetch()` (local daily history) and `server_poll::fetch()` /
  `read_user_info()` (live rate limits + identity) concurrently-in-spirit, merges results into the
  `Mutex<AppState>` managed by Tauri, and emits an `"usage-updated"` event with the full state to
  every webview. The frontend never polls — `App.tsx` just calls `get_usage_data` once on mount and
  then listens for that event (`src/App.tsx`).
- `refresh()` is triggered from three places in `src-tauri/src/lib.rs::run()`: once at startup, from
  a filesystem watcher on `~/.claude/projects/` (debounced, `watcher.rs`), and from a 1-second-tick
  loop that fires on the configured `server_poll_interval_s` *or* immediately if it detects a
  wall-clock jump bigger than 5s (i.e. the machine just woke from sleep) — see the "woke_from_sleep"
  check in `lib.rs`. This sleep-detection pattern is deliberate; don't replace the loop with a plain
  `sleep(interval)` or wake behavior regresses.
- Two independent data sources feed `AppState` (`src-tauri/src/data.rs`):
  - `ccusage.rs` shells out to the bundled sidecar (`ccusage daily --json --offline --since <date>`)
    for per-day token/cost history. It also passes `--config resources/ccusage-pricing.json` — a
    bundled pricing override for models newer than ccusage's own offline pricing snapshot (e.g.
    claude-sonnet-5), so cost doesn't silently show `$0` for those models. Update that JSON when new
    models ship faster than upstream ccusage's pricing data.
  - `server_poll.rs` makes a minimal 1-token `/v1/messages` call to read
    `anthropic-ratelimit-unified-{5h,7d}-*` response headers — this is the *only* accurate live
    source for the rolling rate-limit windows. OAuth token is read from `CLAUDE_CODE_OAUTH_TOKEN` or
    `~/.claude/.credentials.json`. Do not increase poll frequency casually; it's tuned to stay well
    under 0.3% of a 5h Pro quota window.
- `crate::taskbar_overlay::update()` is called at the end of every `refresh()` with a computed
  staleness flag (`OVERLAY_STALE_AFTER_SECS = 180`) — the overlay has its own idea of "stale" and
  doesn't just mirror `state.error`.

### Windows and UI surfaces

- `popup` (`index.html` → `App.tsx`) and `widget` (`widget.html` → `Widget.tsx`) are both built by
  Vite as separate rollup inputs (`vite.config.ts`). They are mutually exclusive by design: showing
  one hides the other (`commands::show_popup` / `commands::toggle_widget` in `commands.rs`, plus the
  tray menu handlers in `tray.rs`). When adding a UI feature, decide up front which surface(s) it
  belongs to — most Dashboard/History/Settings logic lives only in the popup.
- The tray icon (`tray.rs`) positions the popup itself above the tray icon on click, with a
  blur-hide + "just hidden" suppression window (300ms) to stop the same click from re-opening a
  window it just closed. If you touch tray click handling, preserve that debounce or clicking the
  tray icon will flicker.
- The taskbar overlay (`src-tauri/src/taskbar_overlay.rs`, ~1200 lines) is a **native Win32 layered
  window per monitor**, not a Tauri WebView — it runs on its own thread with a blocking `GetMessage`
  loop specifically to keep idle CPU near zero. It owns its own GDI drawing (via `tiny-skia` +
  `ab_glyph` for text), topmost z-order re-assertion against Explorer, DPI/monitor-hotplug handling,
  and a small context menu. This module is Windows-only (`#[cfg(windows)] mod imp`) and is dense —
  read the module doc-comment at the top before editing, and re-run the relevant checklist in
  `docs/soak-test.md` for any change here (renders-only-on-change, idle CPU, GDI handle count,
  sleep/wake, taskbar move/resize all have specific pass criteria).
- `src/ticker.tsx` provides a single shared 1-second clock (`TickerProvider`/`useNow`) used by every
  countdown/"updated Xs ago" display, and pauses via the Page Visibility API while the window is
  hidden. Don't add ad hoc `setInterval` timers for clock-like UI — use this context so background
  windows stay at ~0% CPU.

### Settings

`Settings` (`src-tauri/src/data.rs`) is the single persisted config struct, JSON-serialized to
`{app_data_dir}/settings.json` (`commands::load_settings_from_disk` / `save_settings_to_disk`). It's
also mirrored in `Mutex<Settings>` app state and in `src/types.ts` on the frontend — when adding a
setting, update all three (Rust struct + `Default` impl, `settings.json` shape is implicit, and the
TS interface), plus wire any side effect it triggers (autostart, widget visibility, taskbar overlay)
into `commands::save_settings`, which is the single point where settings changes are applied to
running state, not just persisted.

### Sidecar/bundling notes

- The bundled ccusage binary is an *external binary* (Tauri sidecar), declared in
  `src-tauri/tauri.conf.json` (`bundle.externalBin`) and permissioned in
  `src-tauri/capabilities/default.json` (`shell:allow-execute` scoped to `binaries/ccusage`). Any
  change to how the sidecar is invoked needs to stay consistent across `build-sidecar.mjs` (staging),
  `tauri.conf.json` (bundling), and `capabilities/default.json` (permission), or release builds will
  fail even though `cargo tauri dev` works from your locally-staged binary.
- `dist/` is a committed build output directory (referenced by `tauri.conf.json` as
  `frontendDist`) — don't hand-edit files there; they're regenerated by `pnpm build`.

### Cross-cutting conventions

- Rust modules are declared in `src-tauri/src/lib.rs` and are each single-purpose (one data source,
  one OS integration, or one concern) — follow that when adding a module rather than growing an
  existing file across concerns.
- `AppState` and `Settings` are the only two `Mutex`-managed Tauri states; both are `Clone` and get
  snapshotted (not locked-and-held) before being sent across threads/emitted as events — keep that
  pattern for any new shared state to avoid holding a lock across an `await`/IPC boundary.
- Frontend format helpers (`fmtTokens`, `fmtPct`, `fmtCost`, `fmtAgo`, `fmtCountdown`, `gaugeColor`)
  live in `src/types.ts` next to the interfaces they format — reuse them instead of re-deriving
  formatting in components.
