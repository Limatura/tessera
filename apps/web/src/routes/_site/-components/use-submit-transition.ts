import { toast } from "@tessera/ui/components/toast";
import type { UploadResultData } from "@tessera/upload";
import { useRef, useState } from "react";

import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

export type SubmitStatus = "idle" | "uploading" | "done";

const EXPAND_EASE = [0.32, 0.72, 0, 1] as const;

/**
 * The circular reveal's timing. Expanding answers the click, so it leaves fast
 * and settles slowly (ease-out); collapsing gathers pace toward the button.
 */
export const EXPAND_TRANSITION = { duration: 0.65, ease: EXPAND_EASE };
export const COLLAPSE_TRANSITION = {
  duration: 0.6,
  ease: [0.65, 0, 0.35, 1] as const,
};

/** Centre and radius of the circular reveal, just large enough to cover the viewport */
export interface Origin {
  x: number;
  y: number;
  radius: number;
}

/** One axis of a CSS cubic-bezier with end points 0 and 1, at parameter `s`. */
const bezierAxis = (p1: number, p2: number, s: number) =>
  3 * p1 * s * (1 - s) ** 2 + 3 * p2 * s ** 2 * (1 - s) + s ** 3;

/**
 * Seconds after the expand starts at which the circle's edge reaches a point
 * `distance` px from its centre: finds where the eased progress equals
 * distance / radius, then reads the time at that spot of the curve.
 */
export function revealArrival(origin: Origin, distance: number): number {
  const target = Math.min(1, distance / origin.radius);
  const [x1, y1, x2, y2] = EXPAND_EASE;
  // y rises monotonically along this curve, so bisection on s is safe
  let low = 0;
  let high = 1;
  for (let i = 0; i < 20; i += 1) {
    const s = (low + high) / 2;
    if (bezierAxis(y1, y2, s) < target) {
      low = s;
    } else {
      high = s;
    }
  }
  return bezierAxis(x1, x2, (low + high) / 2) * EXPAND_TRANSITION.duration;
}

/** The circle that grows from the button's centre to just cover the viewport */
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

/**
 * Owns the submit interaction shared by every upload tab: on click, `run`
 * calls the API and reports progress, and when it finishes a circle grows
 * from the button (see `origin`) to reveal the result.
 *
 * States: idle -> (start) -> uploading -> done -> expanded: true
 *         expanded: true -> (reset) -> expanded: false -> idle
 *         uploading -> (failure) -> idle, with a toast
 */
export function useSubmitTransition() {
  const [status, setStatus] = useState<SubmitStatus>("idle");
  const [progress, setProgress] = useState(0);
  const [origin, setOrigin] = useState<Origin | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [result, setResult] = useState<UploadResultData | null>(null);
  // Set by reset: onAnimationComplete also fires when the overlay mounts at
  // circle(0) during upload and after the expand, and neither may clear state.
  const collapsingRef = useRef(false);

  // Takes the clicked button (where the circle starts) and the upload function.
  // Ignored while a submit is already in flight (status !== "idle").
  const start = async (
    el: HTMLElement | null,
    run: (onProgress: (percent: number) => void) => Promise<UploadResultData>
  ) => {
    if (status !== "idle" || !el) {
      return;
    }
    setOrigin(getExpandOrigin(el));
    setStatus("uploading");
    setProgress(0);

    try {
      const data = await run(setProgress);
      setResult(data);
      setStatus("done");
      setTimeout(() => setExpanded(true), 250);
    } catch (error) {
      setStatus("idle");
      setProgress(0);
      setOrigin(null);
      if (!(error instanceof DOMException && error.name === "AbortError")) {
        toast.add({
          description: apiErrorMessage(error, m.common_try_again()),
          title: m.upload_failed(),
          type: "error",
        });
      }
    }
  };

  // Close the reveal first (expanded: false plays the circular collapse).
  // State is cleared only once the overlay reports the collapse finished
  // (`collapsed`, wired to onAnimationComplete): clearing early nulls origin,
  // AnimatePresence takes over the exit midway and unmounts the overlay's
  // canvas and result panel at once, and that burst of DOM teardown lands on
  // frames still animating. A timer of the same length is not enough — the
  // animation starts a frame or two after setExpanded, so the timer fires
  // just before the end and the last frames stutter.
  const reset = () => {
    collapsingRef.current = true;
    setExpanded(false);
  };

  const collapsed = () => {
    if (!collapsingRef.current) {
      return;
    }
    collapsingRef.current = false;
    setStatus("idle");
    setProgress(0);
    setOrigin(null);
    setResult(null);
  };

  return {
    collapsed,
    expanded,
    origin,
    progress,
    reset,
    result,
    start,
    status,
  };
}
