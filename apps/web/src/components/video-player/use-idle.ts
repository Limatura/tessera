import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Goes idle `delay` ms after the last `wake()`, so the controls can get out of
 * the picture. Never idle while `enabled` is false (paused, a menu open, a
 * drag on the timeline).
 */
export function useIdle(enabled: boolean, delay: number) {
  const [idle, setIdle] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const schedule = useCallback(() => {
    if (timer.current) {
      clearTimeout(timer.current);
    }
    timer.current = enabled ? setTimeout(() => setIdle(true), delay) : null;
  }, [enabled, delay]);

  const wake = useCallback(() => {
    setIdle(false);
    schedule();
  }, [schedule]);

  useEffect(() => {
    schedule();
    return () => {
      if (timer.current) {
        clearTimeout(timer.current);
      }
    };
  }, [schedule]);

  return { idle: enabled && idle, sleep: () => setIdle(true), wake };
}
