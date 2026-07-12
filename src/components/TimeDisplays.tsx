import { memo } from "react";
import { useNow } from "../ticker";
import { fmtAgo, fmtCountdown } from "../types";

// Leaf components that subscribe to the shared ticker so only these small
// nodes re-render every second, not the dashboard/chart trees around them.

export const Countdown = memo(function Countdown({ resetTs }: { resetTs?: number }) {
  useNow();
  if (!resetTs) return null;
  return <>{fmtCountdown(resetTs)}</>;
});

export const Ago = memo(function Ago({ ts }: { ts?: number }) {
  useNow();
  return <>{fmtAgo(ts)}</>;
});
