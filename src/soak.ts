import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

const FIVE_MINUTES_MS = 5 * 60 * 1000;

interface PerformanceMemory {
  usedJSHeapSize: number;
}

// Dev-only soak-test logger, active only when the backend has UT_SOAK_LOG=1.
export function useSoakLogger() {
  useEffect(() => {
    let interval: number | null = null;
    let cancelled = false;

    invoke<boolean>("soak_enabled").then(enabled => {
      if (!enabled || cancelled) return;
      const tick = () => {
        const memory = (performance as Performance & { memory?: PerformanceMemory }).memory;
        invoke("soak_log", { jsHeapBytes: memory?.usedJSHeapSize ?? 0 }).catch(console.error);
      };
      tick();
      interval = window.setInterval(tick, FIVE_MINUTES_MS);
    }).catch(console.error);

    return () => {
      cancelled = true;
      if (interval != null) window.clearInterval(interval);
    };
  }, []);
}
