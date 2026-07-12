import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";

// Single 1 second ticker shared by every countdown/ago display in this window.
// Paused entirely while the window is hidden, so a background popup or widget
// costs nothing on a long-running session.

const TickerContext = createContext<number>(Math.floor(Date.now() / 1000));

export function useNow(): number {
  return useContext(TickerContext);
}

export function TickerProvider({ children }: { children: ReactNode }) {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  const intervalRef = useRef<number | null>(null);

  useEffect(() => {
    const start = () => {
      if (intervalRef.current != null) return;
      if (import.meta.env.DEV) console.debug("[ticker] started");
      setNow(Math.floor(Date.now() / 1000));
      intervalRef.current = window.setInterval(() => {
        setNow(Math.floor(Date.now() / 1000));
      }, 1000);
    };
    const stop = () => {
      if (intervalRef.current == null) return;
      window.clearInterval(intervalRef.current);
      intervalRef.current = null;
      if (import.meta.env.DEV) console.debug("[ticker] stopped");
    };

    const onVisibility = () => {
      if (document.hidden) stop();
      else start();
    };

    if (!document.hidden) start();
    document.addEventListener("visibilitychange", onVisibility);

    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      stop();
    };
  }, []);

  return <TickerContext.Provider value={now}>{children}</TickerContext.Provider>;
}
