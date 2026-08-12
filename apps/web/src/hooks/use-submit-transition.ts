import { useState } from "react";

export type SubmitStatus = "idle" | "uploading" | "done";

interface Origin {
  x: number;
  y: number;
  radius: number;
}

const UPLOAD_DURATION_MS = 1400;

function getExpandOrigin(el: HTMLElement): Origin {
  const rect = el.getBoundingClientRect();
  const x = rect.left + rect.width / 2;
  const y = rect.top + rect.height / 2;
  const radius = Math.hypot(
    Math.max(x, window.innerWidth - x),
    Math.max(y, window.innerHeight - y)
  );
  return { radius, x, y };
}

export function useSubmitTransition() {
  const [status, setStatus] = useState<SubmitStatus>("idle");
  const [progress, setProgress] = useState(0);
  const [origin, setOrigin] = useState<Origin | null>(null);
  const [expanded, setExpanded] = useState(false);

  const start = (el: HTMLElement | null) => {
    if (status !== "idle" || !el) {
      return;
    }
    setOrigin(getExpandOrigin(el));
    setStatus("uploading");
    setProgress(0);

    const startTime = performance.now();
    const tick = (now: number) => {
      const elapsed = now - startTime;
      const next = Math.min(100, (elapsed / UPLOAD_DURATION_MS) * 100);
      setProgress(next);
      if (next < 100) {
        requestAnimationFrame(tick);
      } else {
        setStatus("done");
        setTimeout(() => setExpanded(true), 250);
      }
    };
    requestAnimationFrame(tick);
  };

  const reset = () => {
    setExpanded(false);
    setTimeout(() => {
      setStatus("idle");
      setProgress(0);
      setOrigin(null);
    }, 500);
  };

  return { expanded, origin, progress, reset, start, status };
}
