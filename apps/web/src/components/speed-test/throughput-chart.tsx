import { cn } from "@tessera/ui/lib/utils";

export interface ThroughputPoint {
  /** Seconds since this direction started. */
  t: number;
  mbps: number;
}

const WIDTH = 240;
const HEIGHT = 48;

/** A small area chart of the rate over one direction, drawn as the transfer goes. */
export function ThroughputChart({
  points,
  className,
}: {
  points: ThroughputPoint[];
  className?: string;
}) {
  const last = points.at(-1);
  if (!last || points.length < 2) {
    return (
      <div
        aria-hidden="true"
        className={cn("h-12 border-border border-b border-dashed", className)}
      />
    );
  }
  const span = Math.max(last.t, 0.001);
  const peak = Math.max(...points.map((p) => p.mbps), 1);
  const coords = points.map(
    (p) =>
      [
        (p.t / span) * WIDTH,
        HEIGHT - 2 - (p.mbps / peak) * (HEIGHT - 6),
      ] as const
  );
  const line = coords
    .map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`)
    .join(" ");
  const area = `0,${HEIGHT} ${line} ${WIDTH},${HEIGHT}`;

  return (
    <svg
      aria-hidden="true"
      className={cn("h-12 w-full overflow-visible", className)}
      preserveAspectRatio="none"
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
    >
      <polygon className="fill-primary/15" points={area} />
      <polyline
        className="stroke-primary"
        fill="none"
        points={line}
        strokeLinejoin="round"
        strokeWidth={2}
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}
