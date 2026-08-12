import { BlobCanvas } from "./blob-canvas";
import type { BlobSpec } from "./blob-canvas";

const AMBIENT_SPECS: BlobSpec[] = [
  {
    alpha: 0.22,
    baseRadius: 0.32,
    colorVar: "--chart-1",
    cx: 0.28,
    cy: 0.32,
    driftAmp: 0.04,
    driftPhase: 0,
    driftSpeed: 0.06,
  },
  {
    alpha: 0.2,
    baseRadius: 0.3,
    colorVar: "--primary",
    cx: 0.72,
    cy: 0.68,
    driftAmp: 0.045,
    driftPhase: 2.1,
    driftSpeed: 0.05,
  },
  {
    alpha: 0.16,
    baseRadius: 0.26,
    colorVar: "--chart-1",
    cx: 0.6,
    cy: 0.25,
    driftAmp: 0.035,
    driftPhase: 4.2,
    driftSpeed: 0.07,
  },
];

interface BlobBackgroundProps {
  className?: string;
  /** Fades the edges toward `--color-background`. Disable when embedded in a panel with its own solid background. */
  vignette?: boolean;
}

export function BlobBackground({
  className,
  vignette = true,
}: BlobBackgroundProps) {
  return (
    <div
      aria-hidden="true"
      className={
        className ??
        "pointer-events-none fixed inset-0 -z-10 overflow-hidden bg-background"
      }
    >
      <BlobCanvas className="h-full w-full" specs={AMBIENT_SPECS} />
      {vignette && (
        <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_center,transparent_0%,var(--color-background)_92%)]" />
      )}
    </div>
  );
}
