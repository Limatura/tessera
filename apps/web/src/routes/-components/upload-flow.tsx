import { AnimatePresence, motion } from "motion/react";
import { useRef, useState } from "react";

import { UploadButton } from "./upload-button";
import { UploadResult } from "./upload-result";

type Status = "idle" | "uploading" | "done";

interface Origin {
  x: number;
  y: number;
  radius: number;
}

const UPLOAD_DURATION_MS = 1400;

function getExpandOrigin(button: HTMLButtonElement): Origin {
  const rect = button.getBoundingClientRect();
  const x = rect.left + rect.width / 2;
  const y = rect.top + rect.height / 2;
  const radius = Math.hypot(
    Math.max(x, window.innerWidth - x),
    Math.max(y, window.innerHeight - y)
  );
  return { radius, x, y };
}

export function UploadFlow() {
  const [status, setStatus] = useState<Status>("idle");
  const [progress, setProgress] = useState(0);
  const [origin, setOrigin] = useState<Origin | null>(null);
  const [expanded, setExpanded] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const handleUpload = () => {
    if (status !== "idle" || !buttonRef.current) {
      return;
    }
    setOrigin(getExpandOrigin(buttonRef.current));
    setStatus("uploading");
    setProgress(0);

    const start = performance.now();
    const tick = (now: number) => {
      const elapsed = now - start;
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

  const handleReset = () => {
    setExpanded(false);
    setTimeout(() => {
      setStatus("idle");
      setProgress(0);
      setOrigin(null);
    }, 500);
  };

  return (
    <div className="relative flex min-h-svh w-full items-center justify-center bg-background p-6">
      <UploadButton
        onClick={handleUpload}
        progress={progress}
        ref={buttonRef}
        status={status}
      />

      <AnimatePresence>
        {origin && (
          <motion.div
            animate={{
              clipPath: expanded
                ? `circle(${origin.radius}px at ${origin.x}px ${origin.y}px)`
                : `circle(0px at ${origin.x}px ${origin.y}px)`,
            }}
            className="fixed inset-0 z-50 flex items-center justify-center bg-primary"
            exit={{
              clipPath: `circle(0px at ${origin.x}px ${origin.y}px)`,
            }}
            initial={{
              clipPath: `circle(0px at ${origin.x}px ${origin.y}px)`,
            }}
            transition={{ duration: 0.7, ease: [0.65, 0, 0.35, 1] }}
          >
            {expanded && <UploadResult onReset={handleReset} />}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
