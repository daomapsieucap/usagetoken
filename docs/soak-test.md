# Soak test: 8+ hour uptime

This checklist validates that UsageToken stays smooth (tray popup, always-on-top
mini widget, dragging, charts, bridge updates, OAuth polling) across a full
workday of continuous uptime, including sleep/lock-screen cycles.

## Running it

1. Launch the app normally and leave it running for at least 8 hours with
   normal daily use (open/close the popup, toggle the widget, let the machine
   sleep and wake at least once).
2. To also capture memory samples over time, launch with the soak logger
   enabled:

   ```
   $env:UT_SOAK_LOG = "1"
   usagetoken.exe
   ```

   Every 5 minutes this appends a line to `soak.log` in the app data
   directory (`%APPDATA%\com.daomapsieucap.usagetoken\soak.log`) with
   the process working set size and the popup window's JS heap size:

   ```
   ts=1752275000 working_set_bytes=41932800 js_heap_bytes=8321024
   ```

   Leave it disabled (default) for normal runs - it does nothing unless
   `UT_SOAK_LOG=1` is set.

## Pass criteria (after an 8 hour continuous run)

1. Total RAM (main process plus WebView2 children) stays within 10 MB of the
   30 minute baseline. No steady upward slope. If `soak.log` was collected,
   plot `working_set_bytes` over time to confirm it is flat, not sloped.
2. Widget drag remains frame-smooth at hour 8 (native drag makes this
   automatic; the check is that nothing regressed it).
3. Popup open latency at hour 8 is indistinguishable from launch (target
   under 300 ms to first content since data renders from cache).
4. CPU averages 0.0 to 0.1 percent idle at hour 8.
5. No zombie `ccusage` or bridge child processes in Task Manager.
6. Source indicator (server error banner / stale "updated Xs ago" text)
   transitions correctly through at least one fresh-data, stale-data, and
   wake-from-sleep cycle during the day.
7. Countdown timers ("resets in Xm") show correct values immediately after
   wake from sleep, not a frozen pre-sleep value.

## What to check for each criterion

- **RAM (1)**: Task Manager -> Details -> sum the app's processes (main +
  WebView2 renderer/GPU helpers), or read `working_set_bytes` from
  `soak.log` if the logger was enabled.
- **Drag (2)**: drag the mini widget by its title bar; it should track the
  cursor with no visible lag, the same as right after launch.
- **Popup latency (3)**: click the tray icon and time to first paint;
  compare a stopwatch measurement at hour 0 and hour 8.
- **CPU (4)**: Task Manager -> Details -> CPU column, sampled while idle
  (no popup interaction) for ~30 seconds.
- **Zombie processes (5)**: Task Manager -> search for `ccusage`,
  `ccusage.cmd`, or extra `usagetoken.exe` instances after the app has been
  idle for a while.
- **Source indicator (6)**: temporarily block network access (or let the
  OAuth token expire) to see the error banner appear, then restore access
  and confirm it clears on the next poll; also check behavior right after a
  sleep/wake cycle.
- **Countdown (7)**: note the displayed reset countdown before closing the
  laptop lid, then check it immediately after waking - it should reflect
  the real elapsed time, not have paused or frozen.

## Taskbar overlay (optional feature)

The taskbar overlay is a native Win32 layered window per monitor (not a
WebView), so it needs its own soak pass when enabled (Settings -> "Taskbar
overlay").

1. Enable the overlay, launch with `UT_SOAK_LOG=1` set, and leave it running
   for 8+ hours with normal daily use (sleep/wake at least once, and
   plug/unplug a second monitor if available).
2. This appends to a separate `overlay-soak.log` in the app data directory
   (`%APPDATA%\com.daomapsieucap.usagetoken\overlay-soak.log`), one line per
   render and per reposition:

   ```
   ts=1752275000 render monitor=0x10048 pct_5h=61 pct_7d=63 stale=false dur_us=1689
   ts=1752275000 reposition monitor=0x10048 x=1578 y=1044 w=92 h=24 edge=Some(Bottom) hidden=false
   ```

### Pass criteria

1. **Renders only on value changes** - grep `overlay-soak.log` for `render`
   lines; the count over 8 hours should roughly match the number of distinct
   `(pct_5h, pct_7d, stale)` transitions the server-poll cycle produced, not
   one per poll tick.
2. **Idle CPU stays ~0%** - the overlay thread blocks on `GetMessage`; sampled
   CPU for the whole process should look the same as with the overlay
   disabled.
3. **GDI handle count flat** - Task Manager -> Details -> add the "GDI
   objects" column for `usage-token.exe`; it should stay flat after the
   initial per-monitor window/DIB creation (one `HDC` + one `HBITMAP` per
   visible monitor overlay, recreated only on a DPI change, not per render).
4. **Sleep/wake and monitor hotplug**: unplug/replug a monitor, or suspend
   and resume the laptop; overlays should reappear correctly positioned
   within ~1s (driven by `WM_DISPLAYCHANGE`, debounced 500ms) - check for
   `monitor-removed` / `reposition` lines around the event.
5. **Taskbar moved or resized**: drag the taskbar to a different screen edge,
   or resize it; the pill should reposition within ~1s and the `edge` field
   in the log should reflect the new orientation.
6. **Toggling the setting off** removes all overlay windows immediately and
   the thread exits - confirm no leftover `usage-token.exe`-owned windows in
   a tool like Spy++/Accessibility Insights, and that GDI/handle count in
   Task Manager drops back to the pre-enable baseline.

Known limitation: an auto-hidden taskbar's transient hover show/hide is not
tracked live (that would require polling, which conflicts with the "no
timers besides the debounce" requirement). The overlay does correctly hide
when the taskbar is auto-hidden and re-evaluates on the same events as
everything else (display/settings/DPI change).
