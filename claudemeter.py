#!/usr/bin/env python3
"""
claudemeter.py — Claude Code usage tracker for the Windows system tray.

PRIMARY data: server-side remaining-capacity from Anthropic's unified rate-limit
headers (via a minimal inference call). Covers the shared Pro limit across
claude.ai, Claude Code, and Claude Desktop.

SECONDARY data: local token-consumption estimates from ccusage (offline JSONL).
"""
from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
import threading
import time
from datetime import datetime, timedelta
from pathlib import Path
import tkinter as tk
import tkinter.font as tkfont

import usage_provider

# ── Dependency check ──────────────────────────────────────────────────────────
_MISSING: list[str] = []
try:
    import pystray
    from PIL import Image, ImageDraw
except ImportError:
    _MISSING.append("pystray Pillow")

try:
    from watchdog.observers import Observer
    from watchdog.events import FileSystemEventHandler
    _WATCHDOG = True
except ImportError:
    _WATCHDOG = False

if _MISSING:
    print(f"Missing packages. Run:  pip install {' '.join(_MISSING)}")
    sys.exit(1)

# ── Constants ─────────────────────────────────────────────────────────────────
CLAUDE_DIR    = Path.home() / ".claude" / "projects"
FALLBACK_SEC  = 60       # safety-fallback refresh interval (also OAuth poll interval)
DEBOUNCE_SEC  = 2.5      # file-event debounce delay
WIN_W, WIN_H  = 540, 780
APP_DIR       = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent))

# Gauge color thresholds — applied to percent REMAINING
GAUGE_GREEN   = 50    # >= 50% remaining → green
GAUGE_AMBER   = 20    # >= 20% remaining → amber; below → red

# Circle gauge sizes (px)
SRV_CIRC  = 140
BLK_CIRC  = 115

# ── Palette — dao light mode + Fluent Design ──────────────────────────────────
BG        = "#f3f3f3"
BG2       = "#ffffff"
BG3       = "#e5e5e5"
ACC       = "#f3c02c"
ACC2      = "#c79412"
FG        = "#1d3450"
FG2       = "#6c7480"
BLUE      = "#3a5a8c"
RED       = "#dc2626"
ORANGE    = "#ea580c"
GREEN     = "#16a34a"
AMBER     = "#d97706"
CARD_BDR  = "#e0e0e0"
FONT_MONO_CANDIDATES = (
    "JetBrains Mono",
    "Cascadia Mono",
    "Cascadia Code",
    "Consolas",
    "Courier New",
)
FONT_MONO = FONT_MONO_CANDIDATES[0]
BUNDLED_FONT_FILES = (
    APP_DIR / "assets" / "fonts" / "JetBrainsMono-Regular.ttf",
    APP_DIR / "assets" / "fonts" / "JetBrainsMono-Bold.ttf",
)

# ── Helpers ───────────────────────────────────────────────────────────────────

def fmt_tokens(n: int) -> str:
    if n >= 1_000_000:
        return f"{n / 1_000_000:.2f}M"
    if n >= 1_000:
        return f"{n / 1_000:.1f}K"
    return str(n)

def fmt_percent(n: float) -> str:
    if n >= 100:
        return f"{n:.0f}%"
    if n >= 10:
        return f"{n:.1f}%"
    return f"{n:.2f}%"

def fmt_ago(ts: float | None) -> str:
    if ts is None:
        return "not yet refreshed"
    s = int(time.time() - ts)
    if s < 60:
        return f"updated {s}s ago"
    m = s // 60
    if m < 60:
        return f"updated {m}m ago"
    return f"updated {m // 60}h {m % 60}m ago"

def fmt_countdown(reset_ts: int) -> str:
    """'resets in Xh Ym' countdown from a Unix timestamp."""
    if not reset_ts:
        return ""
    secs_left = reset_ts - int(time.time())
    if secs_left <= 0:
        return "resetting now"
    m = secs_left // 60
    h = m // 60
    m = m % 60
    if h > 0:
        return f"resets in {h}h {m:02d}m"
    return f"resets in {m}m"

def fmt_reset_label(reset_ts: int) -> str:
    """'resets Mon at 22:30' or 'resets today at 22:30'."""
    if not reset_ts:
        return ""
    dt  = datetime.fromtimestamp(reset_ts).astimezone()
    now = datetime.now().astimezone()
    delta = (dt.date() - now.date()).days
    if delta == 0:
        prefix = "today"
    elif delta == 1:
        prefix = "tomorrow"
    else:
        prefix = dt.strftime("%a")
    return f"at {dt.strftime('%H:%M')} ({prefix})"

def to_local_hm(iso: str) -> str:
    dt = datetime.fromisoformat(iso.replace("Z", "+00:00")).astimezone()
    return dt.strftime("%H:%M")

def _no_window() -> int:
    return subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0

def _load_bundled_fonts() -> None:
    if sys.platform != "win32":
        return
    try:
        import ctypes
        add_font_resource = ctypes.windll.gdi32.AddFontResourceExW
        add_font_resource.argtypes = [ctypes.c_wchar_p, ctypes.c_ulong, ctypes.c_void_p]
        add_font_resource.restype = ctypes.c_int
        for font_path in BUNDLED_FONT_FILES:
            if font_path.exists():
                add_font_resource(str(font_path), 0x10, None)  # FR_PRIVATE
    except Exception:
        pass

def _select_mono_font(root: tk.Tk) -> str:
    installed = {family.lower(): family for family in tkfont.families(root)}
    for family in FONT_MONO_CANDIDATES:
        if family.lower() in installed:
            return installed[family.lower()]
    return tkfont.nametofont("TkFixedFont").actual("family")

def _run_ccusage(args: list[str]) -> dict | None:
    ccusage_path = shutil.which("ccusage")
    npx_path     = shutil.which("npx")
    candidates: list[list[str]] = []
    if ccusage_path:
        candidates.append([ccusage_path, *args])
    if npx_path:
        candidates.append([npx_path, "ccusage@latest", *args])
    else:
        candidates.append(["npx", "ccusage@latest", *args])
    for cmd in candidates:
        try:
            r = subprocess.run(
                cmd, capture_output=True, text=True,
                timeout=60, creationflags=_no_window(),
            )
            if r.returncode == 0 and r.stdout.strip():
                return json.loads(r.stdout)
        except Exception:
            continue
    return None

def _run_ccusage_text(args: list[str]) -> str | None:
    ccusage_path = shutil.which("ccusage")
    npx_path     = shutil.which("npx")
    candidates: list[list[str]] = []
    if ccusage_path:
        candidates.append([ccusage_path, *args])
    if npx_path:
        candidates.append([npx_path, "ccusage@latest", *args])
    else:
        candidates.append(["npx", "ccusage@latest", *args])
    for cmd in candidates:
        try:
            r = subprocess.run(
                cmd, capture_output=True, text=True,
                timeout=60, creationflags=_no_window(),
            )
            if r.returncode == 0 and r.stdout.strip():
                return r.stdout
        except Exception:
            continue
    return None

def _extract_percent(value) -> float | None:
    if value is None:
        return None
    if isinstance(value, (int, float)):
        percent = float(value)
        return percent * 100 if 0 < percent <= 1 else percent
    if isinstance(value, str):
        match = re.search(r"(\d+(?:\.\d+)?)\s*%", value)
        if match:
            return float(match.group(1))
        try:
            percent = float(value)
            return percent * 100 if 0 < percent <= 1 else percent
        except ValueError:
            return None
    return None

def _extract_token_limit(block: dict) -> int | None:
    tls = block.get("tokenLimitStatus")
    if isinstance(tls, dict):
        value = tls.get("limit")
        if isinstance(value, (int, float)) and value > 0:
            return int(value)
    for key in (
        "tokenLimit", "limit", "quota", "quotaTokens", "maxTokens",
        "maxTokenLimit", "effectiveLimit", "effectiveTokenLimit",
    ):
        value = block.get(key)
        if isinstance(value, (int, float)) and value > 0:
            return int(value)
        if isinstance(value, str):
            digits = re.sub(r"[^\d]", "", value)
            if digits:
                return int(digits)
    projection = block.get("projection")
    if isinstance(projection, dict):
        for key in ("tokenLimit", "limit", "quota", "maxTokens"):
            value = projection.get(key)
            if isinstance(value, (int, float)) and value > 0:
                return int(value)
    return None

def _extract_block_percent(block: dict) -> float | None:
    tls = block.get("tokenLimitStatus")
    if isinstance(tls, dict):
        percent = _extract_percent(tls.get("percentUsed"))
        if percent is not None:
            return percent
    for key in (
        "percent", "percentage", "usagePercent", "quotaPercent",
        "limitPercent", "percentUsed", "usagePercentage",
    ):
        percent = _extract_percent(block.get(key))
        if percent is not None:
            return percent
    projection = block.get("projection")
    if isinstance(projection, dict):
        for key in (
            "percent", "percentage", "usagePercent", "quotaPercent",
            "limitPercent", "percentUsed", "usagePercentage",
        ):
            percent = _extract_percent(projection.get(key))
            if percent is not None:
                return percent
    limit = _extract_token_limit(block)
    total = block.get("totalTokens", 0)
    if limit and isinstance(total, (int, float)):
        return (float(total) / limit) * 100
    return None

def _block_time_percent(block: dict) -> float | None:
    start_str = block.get("startTime")
    end_str   = block.get("endTime")
    if not start_str or not end_str:
        return None
    try:
        start = datetime.fromisoformat(start_str.replace("Z", "+00:00"))
        end   = datetime.fromisoformat(end_str.replace("Z", "+00:00"))
        now   = datetime.now().astimezone()
        total_secs   = (end - start).total_seconds()
        elapsed_secs = (now - start).total_seconds()
        if total_secs <= 0:
            return None
        return max(0.0, min(elapsed_secs / total_secs * 100, 100.0))
    except Exception:
        return None

def _extract_text_percent(text: str | None) -> float | None:
    if not text:
        return None
    patterns = (
        r"(\d+(?:\.\d+)?)\s*%\s*(?:used|usage|of|limit|quota)",
        r"(?:used|usage|quota|limit)[^\n\r%]{0,80}?(\d+(?:\.\d+)?)\s*%",
        r"(\d+(?:\.\d+)?)\s*%",
    )
    for pattern in patterns:
        match = re.search(pattern, text, flags=re.IGNORECASE)
        if match:
            return float(match.group(1))
    return None

def _draw_rounded_rect(canvas, x1, y1, x2, y2, r, color) -> None:
    if r <= 0 or (x2 - x1) < r * 2:
        canvas.create_rectangle(x1, y1, x2, y2, fill=color, outline="")
        return
    canvas.create_arc(x1, y1, x1+2*r, y1+2*r, start=90,  extent=90,  fill=color, outline="")
    canvas.create_arc(x2-2*r, y1, x2, y1+2*r, start=0,   extent=90,  fill=color, outline="")
    canvas.create_arc(x1, y2-2*r, x1+2*r, y2, start=180, extent=90,  fill=color, outline="")
    canvas.create_arc(x2-2*r, y2-2*r, x2, y2, start=270, extent=90,  fill=color, outline="")
    canvas.create_rectangle(x1+r, y1, x2-r, y2, fill=color, outline="")
    canvas.create_rectangle(x1, y1+r, x2, y2-r, fill=color, outline="")

def _ensure_ccusage_global() -> None:
    if shutil.which("ccusage"):
        return
    npm = shutil.which("npm")
    if not npm:
        return
    try:
        subprocess.run(
            [npm, "install", "-g", "ccusage"],
            capture_output=True, timeout=120, creationflags=_no_window(),
        )
    except Exception:
        pass

def _gauge_color(percent_remaining: float) -> str:
    if percent_remaining >= GAUGE_GREEN:
        return GREEN
    if percent_remaining >= GAUGE_AMBER:
        return AMBER
    return RED

def _draw_circle_gauge(canvas: tk.Canvas, pct: float | None, color: str, size: int, font_size: int) -> None:
    canvas.delete("all")
    pad    = size // 9
    ring_w = size // 10
    x0, y0 = pad, pad
    x1, y1 = size - pad, size - pad
    cx, cy  = size // 2, size // 2
    # Background track
    canvas.create_arc(x0, y0, x1, y1, start=0, extent=359.99,
                      style="arc", width=ring_w, outline=BG3)
    if pct is not None and pct > 0:
        sweep = min(359.9, max(2.0, 360.0 * pct / 100.0))
        canvas.create_arc(x0, y0, x1, y1, start=90, extent=sweep,
                          style="arc", width=ring_w, outline=color)
        txt  = fmt_percent(pct)
        tcol = color
    else:
        txt  = "—" if pct is None else fmt_percent(0)
        tcol = FG2
    canvas.create_text(cx, cy, text=txt,
                       font=(FONT_MONO, font_size, "bold"), fill=tcol)

# ── Snapshot (ccusage / local) ────────────────────────────────────────────────

class Snapshot:
    active_block:  dict | None  = None
    today:         dict | None  = None
    week:          list         = []
    refreshed_at:  float | None = None
    error:         str | None   = None

    def fetch(self) -> None:
        try:
            bdata = _run_ccusage(
                ["blocks", "--json", "--offline", "--active"])
            since = (datetime.now() - timedelta(days=7)).strftime("%Y-%m-%d")
            ddata = _run_ccusage(["daily",  "--json", "--offline", "--since", since])

            if bdata is None:
                node_ok = shutil.which("node") or shutil.which("node.exe")
                if not node_ok:
                    self.error = (
                        "Node.js not found.\n\n"
                        "Install Node.js from https://nodejs.org , then restart."
                    )
                else:
                    self.error = (
                        "ccusage not found and npx failed.\n\n"
                        "Try:  npm install -g ccusage"
                    )
                return

            blocks = [b for b in bdata.get("blocks", []) if not b.get("isGap")]
            self.active_block = next((b for b in blocks if b.get("isActive")), None)
            if self.active_block:
                percent = _extract_block_percent(self.active_block)
                if percent is None:
                    usage_text = _run_ccusage_text(
                        ["blocks", "--offline", "--active", "--token-limit", "max"])
                    percent = _extract_text_percent(usage_text)
                time_based = False
                if percent is None:
                    percent    = _block_time_percent(self.active_block)
                    time_based = percent is not None
                self.active_block["usagePercent"]  = percent
                self.active_block["percentIsTime"] = time_based
                self.active_block["tokenLimit"]    = _extract_token_limit(self.active_block)

            today_s    = datetime.now().strftime("%Y-%m-%d")
            daily_list = ddata.get("daily", []) if ddata else []
            self.today = next((d for d in daily_list if d["period"] == today_s), None)
            self.week  = daily_list

            self.refreshed_at = time.time()
            self.error = None

        except Exception as exc:
            self.error = f"Unexpected error: {exc}"

# ── DataManager ───────────────────────────────────────────────────────────────

class DataManager:
    def __init__(self, on_update) -> None:
        self.snap  = Snapshot()
        self.oauth = usage_provider.UsageData(
            windows=[], representative="five_hour", overall_status="unknown",
            fetched_at=0.0, error="Not yet fetched",
        )
        self._cb  = on_update
        self._dbt: threading.Timer | None = None
        self._obs = None

    def _fetch(self) -> None:
        self.snap.fetch()
        self.oauth = usage_provider.fetch_usage()
        self._cb()

    def refresh_async(self) -> None:
        threading.Thread(target=self._fetch, daemon=True).start()

    def _debounced(self) -> None:
        if self._dbt:
            self._dbt.cancel()
        self._dbt = threading.Timer(DEBOUNCE_SEC, self._fetch)
        self._dbt.daemon = True
        self._dbt.start()

    def start(self) -> None:
        threading.Thread(target=_ensure_ccusage_global, daemon=True).start()
        self.refresh_async()

        if _WATCHDOG and CLAUDE_DIR.exists():
            dm = self

            class _H(FileSystemEventHandler):
                def on_any_event(self_, e):
                    if not e.is_directory:
                        dm._debounced()

            self._obs = Observer()
            self._obs.schedule(_H(), str(CLAUDE_DIR), recursive=True)
            self._obs.start()

        def _loop():
            while True:
                time.sleep(FALLBACK_SEC)
                self.refresh_async()
        threading.Thread(target=_loop, daemon=True).start()

    def stop(self) -> None:
        if self._obs:
            self._obs.stop()
        if self._dbt:
            self._dbt.cancel()

# ── Tray icon ─────────────────────────────────────────────────────────────────

def _tray_img(percent_remaining: float | None = None) -> Image.Image:
    sz  = 64
    img = Image.new("RGBA", (sz, sz), (0, 0, 0, 0))
    d   = ImageDraw.Draw(img)
    d.ellipse([2, 2, sz-2, sz-2], fill="#1d3450")

    if percent_remaining is None:
        # No data — idle arc
        d.arc([11, 11, sz-11, sz-11], start=45, end=315, fill="#f3f3f3", width=8)
    else:
        arc_color = _gauge_color(percent_remaining)
        # Sweep 270° clockwise from bottom-left (start=-135), covering percent_remaining
        sweep = max(4, int(270 * percent_remaining / 100))
        d.arc([10, 10, sz-10, sz-10], start=-135, end=-135+sweep,
              fill=arc_color, width=9)
    return img

def _tray_tooltip(mgr: DataManager) -> str:
    if mgr.oauth.ok:
        pw = mgr.oauth.primary_window
        if pw:
            return f"claudemeter — {fmt_percent(pw.percent_remaining)} remaining ({pw.name})"
    b = mgr.snap.active_block
    if b:
        p = b.get("usagePercent")
        if p is not None:
            return f"claudemeter — {fmt_percent(float(p))} used (local est.)"
    return "claudemeter"

# ── Dashboard ─────────────────────────────────────────────────────────────────

class Dashboard:
    def __init__(self, root: tk.Tk, mgr: DataManager) -> None:
        self.root   = root
        self.mgr    = mgr
        self._built = False

        # Server-capacity primary card
        self._srv_circ_cvs:   tk.Canvas | None = None
        self._srv_circ_pct:   float | None     = None
        self._srv_circ_color: str              = BLUE
        self._srv_sub:        tk.StringVar | None = None
        self._srv_bar_cvs:    tk.Canvas   | None = None
        self._srv_bar_pct:    float | None        = None
        self._srv_w5h:        tk.StringVar | None = None
        self._srv_w7d:        tk.StringVar | None = None
        self._srv_note:       tk.StringVar | None = None
        self._srv_5h_rst:     int                 = 0
        self._srv_7d_rst:     int                 = 0

        # ccusage secondary cards
        self._blk_circ_cvs:   tk.Canvas | None = None
        self._blk_circ_pct:   float | None     = None
        self._blk_circ_color: str              = ACC2
        self._block_usage: tk.StringVar | None = None
        self._block_rst:   tk.StringVar | None = None
        self._block_cost:  tk.StringVar | None = None
        self._block_bar_cvs:     tk.Canvas | None = None
        self._block_bar_percent: float | None     = None
        self._today_frame: tk.Frame     | None = None
        self._week_box:    tk.Frame     | None = None
        self._status_lbl:  tk.Label     | None = None
        self._refresh_btn: tk.Button    | None = None

    # ── Construction ─────────────────────────────────────────────────────────

    def build(self) -> None:
        if self._built:
            return
        self._built = True

        r = self.root
        r.title("claudemeter")
        r.geometry(f"{WIN_W}x{WIN_H}")
        r.configure(bg=BG)
        r.resizable(True, True)
        r.minsize(400, 600)
        r.attributes("-topmost", True)
        r.protocol("WM_DELETE_WINDOW", self.hide)

        try:
            import ctypes
            hwnd = ctypes.windll.user32.GetParent(r.winfo_id())
            ctypes.windll.dwmapi.DwmSetWindowAttribute(
                hwnd, 20, ctypes.byref(ctypes.c_int(0)), 4)
            ctypes.windll.dwmapi.DwmSetWindowAttribute(
                hwnd, 38, ctypes.byref(ctypes.c_int(2)), 4)
            ctypes.windll.dwmapi.DwmSetWindowAttribute(
                hwnd, 33, ctypes.byref(ctypes.c_int(2)), 4)
        except Exception:
            pass

        outer = tk.Frame(r, bg=BG)
        outer.pack(fill="both", expand=True)
        self._build_scrollable(outer)

    def _build_scrollable(self, parent: tk.Frame) -> None:
        """Wrap content in a scrollable canvas."""
        cvs = tk.Canvas(parent, bg=BG, highlightthickness=0)
        vsb = tk.Scrollbar(parent, orient="vertical", command=cvs.yview)
        cvs.configure(yscrollcommand=vsb.set)
        vsb.pack(side="right", fill="y")
        cvs.pack(side="left", fill="both", expand=True)

        sf = tk.Frame(cvs, bg=BG)
        win_id = cvs.create_window((0, 0), window=sf, anchor="nw")

        def _on_frame_configure(_e):
            cvs.configure(scrollregion=cvs.bbox("all"))
        sf.bind("<Configure>", _on_frame_configure)

        def _on_canvas_configure(e):
            cvs.itemconfig(win_id, width=e.width)
        cvs.bind("<Configure>", _on_canvas_configure)

        def _on_mousewheel(e):
            cvs.yview_scroll(int(-1 * (e.delta / 120)), "units")
        cvs.bind_all("<MouseWheel>", _on_mousewheel)

        self._build_content(sf)

    def _build_content(self, sf: tk.Frame) -> None:
        # ── Prompt header ─────────────────────────────────────────────────────
        hdr = tk.Frame(sf, bg=BG)
        hdr.pack(fill="x", padx=14, pady=(14, 6))
        tk.Label(hdr, text="dao@chau:~$ ", font=(FONT_MONO, 9),
                 fg=FG2, bg=BG).pack(side="left")
        tk.Label(hdr, text="claudemeter --watch", font=(FONT_MONO, 9),
                 fg=ACC2, bg=BG).pack(side="left")
        tk.Frame(sf, bg=BG3, height=1).pack(fill="x", padx=14, pady=(0, 10))

        def card(title: str, accent: str = ACC2) -> tk.Frame:
            outer = tk.Frame(sf, bg=BG2, highlightthickness=1, highlightbackground=CARD_BDR)
            outer.pack(fill="x", padx=14, pady=(0, 8))
            tk.Frame(outer, bg=accent, width=3).pack(side="left", fill="y")
            content = tk.Frame(outer, bg=BG2)
            content.pack(side="left", fill="both", expand=True)
            tk.Label(content, text=f"// {title}", font=(FONT_MONO, 8, "bold"),
                     fg=accent, bg=BG2, anchor="w").pack(anchor="w", padx=12, pady=(8, 2))
            inner = tk.Frame(content, bg=BG2)
            inner.pack(fill="x", padx=12, pady=(0, 10))
            return inner

        def lbl(p, text="", font=(FONT_MONO, 10), fg=FG, bg=BG2, **kw) -> tk.Label:
            return tk.Label(p, text=text, font=font, fg=fg, bg=bg, **kw)

        # ── Primary: server capacity card ─────────────────────────────────────
        sc = card("server capacity  ·  claude.ai + Claude Code + Desktop", accent=BLUE)

        self._srv_sub  = tk.StringVar(value="")
        self._srv_w5h  = tk.StringVar(value="")
        self._srv_w7d  = tk.StringVar(value="")
        self._srv_note = tk.StringVar(value="")

        top_row = tk.Frame(sc, bg=BG2)
        top_row.pack(fill="x", anchor="w", pady=(0, 6))

        self._srv_circ_cvs = tk.Canvas(top_row, width=SRV_CIRC, height=SRV_CIRC,
                                        bg=BG2, highlightthickness=0)
        self._srv_circ_cvs.pack(side="left", padx=(0, 14))
        self._srv_circ_cvs.bind(
            "<Configure>",
            lambda _e: _draw_circle_gauge(
                self._srv_circ_cvs, self._srv_circ_pct,
                self._srv_circ_color, SRV_CIRC, 17))

        info_col = tk.Frame(top_row, bg=BG2)
        info_col.pack(side="left", fill="both", expand=True)
        lbl(info_col, textvariable=self._srv_sub,
            font=(FONT_MONO, 9), fg=FG2, anchor="w").pack(anchor="w", pady=(0, 2))
        lbl(info_col, textvariable=self._srv_w5h,
            font=(FONT_MONO, 9, "bold"), fg=FG, wraplength=280).pack(anchor="w", pady=(2, 0))
        lbl(info_col, textvariable=self._srv_w7d,
            font=(FONT_MONO, 9), fg=FG, wraplength=280).pack(anchor="w", pady=(2, 0))
        lbl(info_col, textvariable=self._srv_note,
            font=(FONT_MONO, 8), fg=FG2, wraplength=WIN_W - 220, justify="left"
            ).pack(anchor="w", pady=(4, 0))

        self._srv_bar_cvs = tk.Canvas(sc, height=8, bg=BG2, highlightthickness=0)
        self._srv_bar_cvs.pack(fill="x", pady=(0, 4))
        self._srv_bar_cvs.bind(
            "<Configure>", lambda _e: self._set_server_bar(self._srv_bar_pct, is_remaining=True))

        # ── Section divider: usage details ────────────────────────────────────
        div = tk.Frame(sf, bg=BG)
        div.pack(fill="x", padx=14, pady=(4, 4))
        tk.Frame(div, bg=BG3, height=1).pack(fill="x", pady=(0, 6))
        tk.Label(div, text="// usage details  ·  estimated at API rates  (not actual Pro charges)",
                 font=(FONT_MONO, 7, "bold"), fg=FG2, bg=BG, anchor="w").pack(anchor="w")
        tk.Frame(div, bg=BG3, height=1).pack(fill="x", pady=(6, 0))

        # ── Secondary: current 5-hour block (ccusage) ─────────────────────────
        bc = card("current 5-hour block (local estimate)")
        self._block_usage = tk.StringVar(value="")
        self._block_rst   = tk.StringVar(value="")
        self._block_cost  = tk.StringVar(value="")

        blk_row = tk.Frame(bc, bg=BG2)
        blk_row.pack(fill="x", anchor="w", pady=(0, 4))

        self._blk_circ_cvs = tk.Canvas(blk_row, width=BLK_CIRC, height=BLK_CIRC,
                                         bg=BG2, highlightthickness=0)
        self._blk_circ_cvs.pack(side="left", padx=(0, 12))
        self._blk_circ_cvs.bind(
            "<Configure>",
            lambda _e: _draw_circle_gauge(
                self._blk_circ_cvs, self._blk_circ_pct,
                self._blk_circ_color, BLK_CIRC, 14))

        info_blk = tk.Frame(blk_row, bg=BG2)
        info_blk.pack(side="left", fill="both", expand=True)
        lbl(info_blk, textvariable=self._block_usage,
            font=(FONT_MONO, 9, "bold"), fg=FG).pack(anchor="w", pady=(0, 2))
        lbl(info_blk, textvariable=self._block_rst,
            font=(FONT_MONO, 9), fg=BLUE).pack(anchor="w")
        lbl(info_blk, textvariable=self._block_cost,
            font=(FONT_MONO, 8), fg=FG2).pack(anchor="w", pady=(2, 0))

        self._block_bar_cvs = tk.Canvas(bc, height=6, bg=BG2, highlightthickness=0)
        self._block_bar_cvs.pack(fill="x", pady=(0, 4))
        self._block_bar_cvs.bind(
            "<Configure>", lambda _e: self._set_block_bar(self._block_bar_percent))

        # ── Secondary: today ──────────────────────────────────────────────────
        tc = card("today")
        self._today_frame = tk.Frame(tc, bg=BG2)
        self._today_frame.pack(fill="x", anchor="w")

        # ── Secondary: last-7-days ────────────────────────────────────────────
        self._week_box = card("last 7 days")

        # ── Bottom bar ────────────────────────────────────────────────────────
        tk.Frame(sf, bg=BG3, height=1).pack(fill="x", padx=14, pady=(6, 0))
        bar = tk.Frame(sf, bg=BG)
        bar.pack(fill="x", padx=14, pady=(6, 14))

        self._refresh_btn = tk.Button(
            bar, text="$ refresh", font=(FONT_MONO, 9),
            bg=BG3, fg=FG, relief="flat", cursor="hand2",
            padx=10, pady=4, activebackground=BG2, activeforeground=ACC2,
            disabledforeground=FG2,
            command=self._manual_refresh,
        )
        self._refresh_btn.pack(side="left")
        self._refresh_btn.bind(
            "<Enter>", lambda _: self._refresh_btn.config(bg=BG2, fg=ACC2)
            if self._refresh_btn["state"] == "normal" else None)
        self._refresh_btn.bind(
            "<Leave>", lambda _: self._refresh_btn.config(bg=BG3, fg=FG)
            if self._refresh_btn["state"] == "normal" else None)

        self._status_lbl = tk.Label(
            bar, text="", font=(FONT_MONO, 8), fg=FG2, bg=BG)
        self._status_lbl.pack(side="right")

        if not _WATCHDOG:
            tk.Label(
                sf,
                text=("watchdog not installed — file-watching disabled. "
                      "using 60s safety timer only.  "
                      "pip install watchdog  to enable instant updates."),
                font=(FONT_MONO, 8), fg=FG2, bg=BG,
                wraplength=WIN_W - 28, justify="left",
            ).pack(anchor="w", padx=14, pady=(0, 8))

        self._tick_status()
        self.update_ui()

    # ── Bar helpers ──────────────────────────────────────────────────────────

    def _set_server_bar(self, percent: float | None, is_remaining: bool = True) -> None:
        self._srv_bar_pct = percent
        cvs = self._srv_bar_cvs
        if not cvs:
            return
        cvs.delete("all")
        w = cvs.winfo_width() or (WIN_W - 52)
        h = cvs.winfo_height() or 8
        r = h // 2
        _draw_rounded_rect(cvs, 0, 0, w, h, r, BG3)
        if percent is not None:
            visible = max(0.0, min(percent, 100.0))
            fw = max(r * 2, int(w * (visible / 100)))
            color = _gauge_color(percent) if is_remaining else (
                RED if percent >= 90 else ORANGE if percent >= 70 else ACC2)
            _draw_rounded_rect(cvs, 0, 0, fw, h, r, color)

    def _set_block_bar(self, percent: float | None) -> None:
        self._block_bar_percent = percent
        cvs = self._block_bar_cvs
        if not cvs:
            return
        cvs.delete("all")
        w = cvs.winfo_width() or (WIN_W - 52)
        h = cvs.winfo_height() or 6
        r = h // 2
        _draw_rounded_rect(cvs, 0, 0, w, h, r, BG3)
        if percent is not None:
            visible = max(0.0, min(percent, 100.0))
            fw = max(r * 2, int(w * (visible / 100)))
            color = RED if percent >= 90 else ORANGE if percent >= 70 else ACC2
            _draw_rounded_rect(cvs, 0, 0, fw, h, r, color)

    def _set_server_circle(self, pct: float | None, color: str) -> None:
        self._srv_circ_pct   = pct
        self._srv_circ_color = color
        if self._srv_circ_cvs:
            _draw_circle_gauge(self._srv_circ_cvs, pct, color, SRV_CIRC, 17)

    def _set_block_circle(self, pct: float | None) -> None:
        if pct is not None:
            color = RED if pct >= 90 else ORANGE if pct >= 70 else ACC2
        else:
            color = ACC2
        self._blk_circ_pct   = pct
        self._blk_circ_color = color
        if self._blk_circ_cvs:
            _draw_circle_gauge(self._blk_circ_cvs, pct, color, BLK_CIRC, 14)

    # ── Ticker ────────────────────────────────────────────────────────────────

    def _tick_status(self) -> None:
        if not self._built:
            self.root.after(5_000, self._tick_status)
            return
        if self._status_lbl:
            ts = self.mgr.oauth.fetched_at or self.mgr.snap.refreshed_at
            self._status_lbl.config(text=fmt_ago(ts))
        # Update reset countdowns live
        if self._srv_5h_rst and self._srv_w5h:
            w5h = self.mgr.oauth.primary_window if self.mgr.oauth.ok else None
            if w5h and w5h.name == "5h":
                cnt  = fmt_countdown(self._srv_5h_rst)
                used = 100.0 - w5h.percent_remaining
                self._srv_w5h.set(f"5h window · {fmt_percent(used)} used · {cnt}")
        if self._srv_7d_rst and self._srv_w7d:
            w7d = next((w for w in self.mgr.oauth.windows if w.name == "7d"), None) \
                  if self.mgr.oauth.ok else None
            if w7d:
                cnt  = fmt_countdown(self._srv_7d_rst)
                used = 100.0 - w7d.percent_remaining
                self._srv_w7d.set(f"7d window · {fmt_percent(used)} used · {cnt}")
        self.root.after(5_000, self._tick_status)

    # ── Manual refresh ────────────────────────────────────────────────────────

    def _manual_refresh(self) -> None:
        if self._refresh_btn:
            self._refresh_btn.config(state="disabled", text="$ refreshing...")
        self.mgr.refresh_async()

    # ── update_ui (main-thread only) ─────────────────────────────────────────

    def update_ui(self) -> None:
        if not self._built:
            return
        if self._refresh_btn:
            self._refresh_btn.config(state="normal", text="$ refresh", bg=BG3, fg=FG)

        snap  = self.mgr.snap
        oauth = self.mgr.oauth

        # ── Server capacity card ──────────────────────────────────────────────
        if oauth.ok:
            pw = oauth.primary_window
            if pw:
                pct_rem  = pw.percent_remaining
                pct_used = 100.0 - pct_rem
                self._set_server_circle(pct_used, _gauge_color(pct_rem))
                claim_label = "5-hour window" if oauth.representative == "five_hour" else "7-day window"
                status_label = "" if oauth.overall_status == "allowed" else f"  [{oauth.overall_status}]"
                self._srv_sub.set(f"used  ·  {claim_label}{status_label}")
                self._set_server_bar(pct_used, is_remaining=False)
                self._srv_5h_rst = 0
                self._srv_7d_rst = 0
                lines_5h = lines_7d = ""
                for w in oauth.windows:
                    cnt  = fmt_countdown(w.reset_ts)
                    used = 100.0 - w.percent_remaining
                    line = f"{w.name} window · {fmt_percent(used)} used · {cnt}"
                    if w.name == "5h":
                        lines_5h = line
                        self._srv_5h_rst = w.reset_ts
                    else:
                        lines_7d = line
                        self._srv_7d_rst = w.reset_ts
                self._srv_w5h.set(lines_5h)
                self._srv_w7d.set(lines_7d)
                self._srv_note.set("server-side data  ·  shared limit across all Claude surfaces")
            else:
                self._set_server_circle(None, BLUE)
                self._srv_sub.set("no window data")
                self._set_server_bar(None)
                self._srv_note.set(oauth.error or "")
        else:
            # Fallback: show ccusage in primary position with clear label
            b = snap.active_block
            if b and snap.error is None:
                pct_used = b.get("usagePercent")
                if isinstance(pct_used, (int, float)):
                    self._set_server_circle(float(pct_used), ORANGE)
                    self._srv_sub.set("used (local estimate only — server data unavailable)")
                    self._set_server_bar(float(pct_used), is_remaining=False)
                else:
                    self._set_server_circle(None, BLUE)
                    self._srv_sub.set("local estimate — server data unavailable")
                    self._set_server_bar(None)
            else:
                self._set_server_circle(None, BLUE)
                self._srv_sub.set("server data unavailable")
                self._set_server_bar(None)
            self._srv_w5h.set("")
            self._srv_w7d.set("")
            reason = oauth.error or "unknown error"
            self._srv_note.set(f"server data unavailable: {reason}")

        # ── ccusage block card ────────────────────────────────────────────────
        if snap.error:
            self._set_block_circle(None)
            self._block_usage.set("error")
            self._block_rst.set(snap.error)
            self._block_cost.set("")
            self._set_block_bar(None)
            if self._today_frame:
                for w in self._today_frame.winfo_children():
                    w.destroy()
            self._draw_week([])
            return

        b = snap.active_block
        if b:
            total      = b.get("totalTokens", 0)
            percent    = b.get("usagePercent")
            limit      = b.get("tokenLimit")
            time_based = b.get("percentIsTime", False)
            if isinstance(percent, (int, float)):
                self._set_block_circle(float(percent))
                if time_based:
                    self._block_usage.set(
                        f"$ {fmt_tokens(total)} tokens  ·  block time elapsed")
                else:
                    self._block_usage.set(
                        f"$ {fmt_tokens(total)} / {fmt_tokens(limit)} tokens"
                        if limit else f"$ {fmt_tokens(total)} tokens this block")
                self._set_block_bar(float(percent))
            else:
                self._set_block_circle(None)
                self._block_usage.set(f"$ {fmt_tokens(total)} tokens (percent unavailable)")
                self._set_block_bar(None)
            end = b.get("endTime")
            self._block_rst.set(
                f"$ block window ~{to_local_hm(end)}" if end else "")
            cost = b.get("costUSD", 0.0)
            self._block_cost.set(
                f"  est. API-rate cost: ${cost:.4f}" if cost else "")
        else:
            self._set_block_circle(None)
            self._block_usage.set("")
            self._block_rst.set("$ no active 5-hour block  (claude code is idle).")
            self._block_cost.set("")
            self._set_block_bar(None)

        # ── Today ─────────────────────────────────────────────────────────────
        t = snap.today
        if self._today_frame:
            for w in self._today_frame.winfo_children():
                w.destroy()
            if t:
                inp  = fmt_tokens(t.get("inputTokens",         0))
                out  = fmt_tokens(t.get("outputTokens",        0))
                cr   = fmt_tokens(t.get("cacheReadTokens",     0))
                cw   = fmt_tokens(t.get("cacheCreationTokens", 0))
                tot  = fmt_tokens(t.get("totalTokens",         0))
                cost = t.get("totalCost", 0.0)
                for metric, value in (
                    ("total",       tot),
                    ("input",       inp),
                    ("output",      out),
                    ("cache read",  cr),
                    ("cache write", cw),
                    ("est. cost",   f"${cost:.4f}"),
                ):
                    row = tk.Frame(self._today_frame, bg=BG2)
                    row.pack(fill="x", pady=1)
                    tk.Label(row, text=f"  {metric}", font=(FONT_MONO, 8),
                             fg=FG2, bg=BG2, width=14, anchor="w").pack(side="left")
                    tk.Label(row, text=value, font=(FONT_MONO, 9, "bold"),
                             fg=FG, bg=BG2, anchor="w").pack(side="left")
            else:
                tk.Label(self._today_frame, text="$ no claude code activity today.",
                         font=(FONT_MONO, 9), fg=FG2, bg=BG2).pack(anchor="w")

        self._draw_week(snap.week)

    def _draw_week(self, week_data: list) -> None:
        if not self._week_box:
            return
        for w in self._week_box.winfo_children():
            w.destroy()
        if not week_data:
            tk.Label(self._week_box,
                     text="$ no data for the last 7 days.",
                     font=(FONT_MONO, 9), fg=FG2, bg=BG2).pack(anchor="w")
            return

        max_t   = max(d.get("totalTokens", 0) for d in week_data) or 1
        bar_max = 220

        for d in reversed(week_data):
            total  = d.get("totalTokens", 0)
            bw     = max(0, int(total / max_t * bar_max))
            period = d.get("period", "")
            try:
                label = datetime.strptime(period, "%Y-%m-%d").strftime("%b %d")
            except ValueError:
                label = period

            row = tk.Frame(self._week_box, bg=BG2)
            row.pack(fill="x", pady=2)
            tk.Label(row, text=label, font=(FONT_MONO, 8), fg=FG2, bg=BG2,
                     width=6, anchor="e").pack(side="left")
            track = tk.Frame(row, bg=BG3, width=bar_max, height=8)
            track.pack_propagate(False)
            track.pack(side="left", padx=(6, 4), pady=5)
            if bw > 0:
                tk.Frame(track, bg=ACC2, width=bw).pack(side="left", fill="y")
            tk.Label(row, text=fmt_tokens(total), font=(FONT_MONO, 9),
                     fg=FG, bg=BG2, anchor="w").pack(side="left")

    # ── Visibility ────────────────────────────────────────────────────────────

    def show(self) -> None:
        if not self._built:
            self.build()
        self.root.deiconify()
        self.root.lift()
        self.root.focus_force()

    def hide(self) -> None:
        self.root.withdraw()

# ── App ───────────────────────────────────────────────────────────────────────

class App:
    def __init__(self) -> None:
        global FONT_MONO

        if sys.platform == "win32":
            try:
                import ctypes
                ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID("claudemeter.app")
            except Exception:
                pass

        _load_bundled_fonts()
        self.root = tk.Tk()
        FONT_MONO = _select_mono_font(self.root)

        try:
            from PIL import ImageTk
            self._tk_icon = ImageTk.PhotoImage(_tray_img())
            self.root.iconphoto(True, self._tk_icon)
        except Exception:
            pass

        self.root.withdraw()

        def _on_update() -> None:
            self.root.after(0, self._updated)

        self.mgr  = DataManager(on_update=_on_update)
        self.dash = Dashboard(self.root, self.mgr)

        menu = pystray.Menu(
            pystray.MenuItem("Open Dashboard", self._open, default=True),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("Quit", self._quit),
        )
        self._icon = pystray.Icon(
            "claudemeter", _tray_img(), "claudemeter", menu)

    def _open(self, *_) -> None:
        self.root.after(0, self.dash.show)

    def _updated(self) -> None:
        self.dash.update_ui()
        oauth = self.mgr.oauth
        if oauth.ok and oauth.primary_window:
            pct_rem = oauth.primary_window.percent_remaining
            icon = _tray_img(percent_remaining=pct_rem)
        else:
            b = self.mgr.snap.active_block
            if b:
                p = b.get("usagePercent")
                if isinstance(p, (int, float)):
                    pct_rem = 100.0 - float(p)
                    icon = _tray_img(percent_remaining=max(0.0, pct_rem))
                else:
                    icon = _tray_img()
            else:
                icon = _tray_img()
        try:
            self._icon.icon = icon
            self._icon.title = _tray_tooltip(self.mgr)
        except Exception:
            pass

    def _quit(self, *_) -> None:
        self.root.after(0, self._do_quit)

    def _do_quit(self) -> None:
        self.mgr.stop()
        try:
            self._icon.stop()
        except Exception:
            pass
        self.root.destroy()

    def run(self) -> None:
        self.mgr.start()
        threading.Thread(target=self._icon.run, daemon=True).start()
        self.root.mainloop()


if __name__ == "__main__":
    App().run()
