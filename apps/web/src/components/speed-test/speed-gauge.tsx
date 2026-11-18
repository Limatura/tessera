import { cn } from "@tessera/ui/lib/utils";
import { animate, useMotionValue, useMotionValueEvent } from "motion/react";
import { useEffect, useId, useState } from "react";

import { GAUGE_STOPS, gaugeFraction } from "@/lib/speed-test";

const SIZE = 320;
const CENTER = SIZE / 2;
const RADIUS = 128;
const STROKE = 14;
/** The dial opens at the bottom: it starts at 150° and sweeps 240° clockwise. */
const START_DEG = 150;
const SWEEP_DEG = 240;

function point(deg: number, radius: number) {
  const rad = (deg * Math.PI) / 180;
  return {
    x: CENTER + radius * Math.cos(rad),
    y: CENTER + radius * Math.sin(rad),
  };
}

function arc(radius: number) {
  const from = point(START_DEG, radius);
  const to = point(START_DEG + SWEEP_DEG, radius);
  return `M ${from.x} ${from.y} A ${radius} ${radius} 0 1 1 ${to.x} ${to.y}`;
}

const TRACK = arc(RADIUS);

/**
 * The dial: a 240° arc that fills as the line speeds up, ticks on the usual
 * 5 / 10 / 50 / 100 … scale, and a bead riding the end of the fill that eases toward each new reading.
 */
export function SpeedGauge({
  mbps,
  idle,
  className,
  center,
  children,
}: {
  mbps: number;
  /** Nothing measured yet: the fill and bead stay hidden so `center` has the dial. */
  idle: boolean;
  className?: string;
  /** Sits on the hub, for the start button. */
  center?: React.ReactNode;
  /** Sits in the open gap at the bottom: the reading. */
  children?: React.ReactNode;
}) {
  const gradientId = useId();
  const fraction = useMotionValue(0);
  const [shown, setShown] = useState(0);
  useMotionValueEvent(fraction, "change", setShown);

  useEffect(() => {
    const controls = animate(fraction, gaugeFraction(mbps), {
      damping: 24,
      stiffness: 120,
      type: "spring",
    });
    return () => controls.stop();
  }, [fraction, mbps]);

  const clamped = Math.min(1, Math.max(0, shown));
  const head = point(START_DEG + SWEEP_DEG * clamped, RADIUS);

  return (
    <div
      className={cn(
        "relative mx-auto aspect-square w-full max-w-80",
        className
      )}
    >
      <svg
        aria-hidden="true"
        className="size-full overflow-visible"
        viewBox={`0 0 ${SIZE} ${SIZE}`}
      >
        <defs>
          <linearGradient id={gradientId} x1="0" x2="1" y1="1" y2="0">
            <stop offset="0%" stopColor="var(--primary)" stopOpacity={0.45} />
            <stop offset="100%" stopColor="var(--primary)" />
          </linearGradient>
        </defs>
        <path
          className="stroke-muted"
          d={TRACK}
          fill="none"
          strokeLinecap="round"
          strokeWidth={STROKE}
        />
        <path
          className="transition-opacity duration-300"
          d={TRACK}
          fill="none"
          opacity={idle ? 0 : 1}
          pathLength={1}
          stroke={`url(#${gradientId})`}
          style={{ filter: "drop-shadow(0 0 6px var(--primary))" }}
          strokeDasharray={1}
          strokeDashoffset={1 - clamped}
          strokeLinecap="round"
          strokeWidth={STROKE}
        />
        {GAUGE_STOPS.map((stop) => {
          const deg = START_DEG + SWEEP_DEG * gaugeFraction(stop);
          const inner = point(deg, RADIUS - STROKE - 6);
          const outer = point(deg, RADIUS - STROKE - 14);
          const label = point(deg, RADIUS - STROKE - 28);
          return (
            <g key={stop}>
              <line
                className="stroke-muted-foreground/50"
                strokeWidth={1.5}
                x1={inner.x}
                x2={outer.x}
                y1={inner.y}
                y2={outer.y}
              />
              <text
                className="fill-muted-foreground text-[10px] tabular-nums"
                dominantBaseline="middle"
                textAnchor="middle"
                x={label.x}
                y={label.y}
              >
                {stop}
              </text>
            </g>
          );
        })}
        <circle
          className="fill-background stroke-primary transition-opacity duration-300"
          cx={head.x}
          cy={head.y}
          opacity={idle ? 0 : 1}
          r={STROKE / 2 + 3}
          strokeWidth={4}
          style={{ filter: "drop-shadow(0 0 6px var(--primary))" }}
        />
      </svg>
      {center && (
        <div className="absolute inset-0 flex items-center justify-center">
          {center}
        </div>
      )}
      <div className="pointer-events-none absolute inset-x-0 bottom-[6%] flex flex-col items-center">
        {children}
      </div>
    </div>
  );
}
