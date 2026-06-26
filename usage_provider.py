"""
usage_provider.py — Real server-side remaining-capacity via Anthropic's unified rate-limit headers.

UNOFFICIAL APPROACH: Anthropic has no documented /usage REST endpoint for Pro subscribers.
Instead, this module makes a minimal 1-token inference call to api.anthropic.com and reads
the anthropic-ratelimit-unified-* response headers, which report 5-hour and 7-day
rolling-window utilization for the caller's account. These headers reflect usage across
claude.ai, Claude Code, and Claude Desktop — the real shared cap for Pro subscribers.

If Anthropic changes the header names or format, update:
  - POLL_ENDPOINT (the API URL used for the ping call)
  - _parse_headers() (the header parser)
Set POLL_ENDPOINT = "" to disable the feature and fall back to ccusage estimates.

CREDENTIALS (checked in order):
  1. CLAUDE_CODE_OAUTH_TOKEN environment variable
  2. ~/.claude/.credentials.json → .claudeAiOauth.accessToken

TOKEN SCOPES: The token needs at least the user:inference scope.
If the call returns a 401 / scope error, run `claude setup-token` or type `/login`
inside Claude Code to mint a token with current scopes.

COST: Each ping call uses approximately 8 input + 1 output = 9 Pro quota tokens,
which is negligible compared to real session usage.
"""
from __future__ import annotations

import json
import os
import time
import urllib.request
import urllib.error
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

# ── Unofficial endpoint config — update here if the API changes ───────────────
# The usage data is read from response headers, not a dedicated endpoint.
# Set to "" to disable server polling entirely.
POLL_ENDPOINT = "https://api.anthropic.com/v1/messages"
POLL_MODEL    = "claude-haiku-4-5-20251001"
POLL_API_VER  = "2023-06-01"
POLL_INTERVAL = 60   # seconds between server polls while tray is active


@dataclass
class UsageWindow:
    name: str                  # "5h" or "7d"
    utilization: float         # 0.0–1.0 fraction used
    percent_remaining: float   # 0–100
    reset_ts: int              # Unix timestamp of window reset
    status: str                # "allowed" | "rate_limited" | "unknown"


@dataclass
class UsageData:
    windows: list[UsageWindow]
    representative: str        # "five_hour" | "seven_day" — tightest binding window
    overall_status: str        # "allowed" | "rate_limited" | "unknown"
    fetched_at: float          # time.time() at fetch
    error: Optional[str] = None
    raw: dict = field(default_factory=dict)

    @property
    def ok(self) -> bool:
        return self.error is None and bool(self.windows)

    @property
    def primary_window(self) -> Optional[UsageWindow]:
        """Return the window identified as the binding constraint."""
        name_map = {"five_hour": "5h", "seven_day": "7d"}
        target = name_map.get(self.representative)
        for w in self.windows:
            if w.name == target:
                return w
        return self.windows[0] if self.windows else None


def _get_oauth_token() -> Optional[str]:
    """Read OAuth token from env var or ~/.claude/.credentials.json. Never logs token."""
    t = os.environ.get("CLAUDE_CODE_OAUTH_TOKEN")
    if t:
        return t
    creds_path = Path.home() / ".claude" / ".credentials.json"
    if creds_path.exists():
        try:
            data = json.loads(creds_path.read_text(encoding="utf-8"))
            nested = data.get("claudeAiOauth") or {}
            if isinstance(nested, dict):
                tok = nested.get("accessToken")
                if tok:
                    return tok
        except Exception:
            pass
    return None


def _parse_headers(headers: dict) -> UsageData:
    """Parse anthropic-ratelimit-unified-* headers into a UsageData."""
    def _f(key: str) -> Optional[float]:
        v = headers.get(key)
        if v is None:
            return None
        try:
            return float(v)
        except (TypeError, ValueError):
            return None

    def _s(key: str, default: str = "unknown") -> str:
        return headers.get(key, default)

    def _ts(key: str) -> int:
        v = headers.get(key)
        if v is None:
            return 0
        try:
            return int(float(v))
        except (TypeError, ValueError):
            return 0

    windows: list[UsageWindow] = []
    for win in ("5h", "7d"):
        util = _f(f"anthropic-ratelimit-unified-{win}-utilization")
        if util is not None:
            windows.append(UsageWindow(
                name=win,
                utilization=min(1.0, max(0.0, util)),
                percent_remaining=max(0.0, min(100.0, (1.0 - util) * 100)),
                reset_ts=_ts(f"anthropic-ratelimit-unified-{win}-reset"),
                status=_s(f"anthropic-ratelimit-unified-{win}-status"),
            ))

    representative = _s("anthropic-ratelimit-unified-representative-claim", "five_hour")
    overall = _s("anthropic-ratelimit-unified-status", "unknown")

    return UsageData(
        windows=windows,
        representative=representative,
        overall_status=overall,
        fetched_at=time.time(),
        error=None if windows else "No unified rate-limit headers in response (API format may have changed)",
        raw=dict(headers),
    )


def fetch_usage() -> UsageData:
    """
    Make a minimal inference call and return server-side usage from response headers.

    Always returns a UsageData. Check .ok / .error before using .windows.
    Never raises — all errors are returned as UsageData with .error set.
    """
    _now = time.time

    if not POLL_ENDPOINT:
        return UsageData(
            windows=[], representative="five_hour", overall_status="unknown",
            fetched_at=_now(), error="Server polling disabled (POLL_ENDPOINT is empty)",
        )

    token = _get_oauth_token()
    if not token:
        return UsageData(
            windows=[], representative="five_hour", overall_status="unknown",
            fetched_at=_now(),
            error="OAuth token not found — set CLAUDE_CODE_OAUTH_TOKEN or run /login in Claude Code",
        )

    body = json.dumps({
        "model": POLL_MODEL,
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "h"}],
    }).encode()

    req = urllib.request.Request(
        POLL_ENDPOINT,
        data=body,
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
            "anthropic-version": POLL_API_VER,
            "User-Agent": "claudemeter/1.0",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=20) as resp:
            hdrs = {k.lower(): v for k, v in resp.headers.items()}
            return _parse_headers(hdrs)
    except urllib.error.HTTPError as e:
        # Headers may still be present on error responses
        hdrs = {k.lower(): v for k, v in e.headers.items()}
        parsed = _parse_headers(hdrs)
        if parsed.ok:
            return parsed
        if e.code == 401:
            msg = "401 — token expired or missing scope; run `claude setup-token` or /login"
        elif e.code == 403:
            msg = "403 — access denied; try refreshing your token"
        elif e.code == 429:
            msg = "429 — already rate-limited by server"
        else:
            try:
                detail = e.read().decode("utf-8", errors="replace")[:120]
            except Exception:
                detail = ""
            msg = f"HTTP {e.code}" + (f": {detail}" if detail else "")
        return UsageData(
            windows=[], representative="five_hour", overall_status="unknown",
            fetched_at=_now(), error=msg,
        )
    except Exception as e:
        return UsageData(
            windows=[], representative="five_hour", overall_status="unknown",
            fetched_at=_now(), error=str(e) or "Network error",
        )
