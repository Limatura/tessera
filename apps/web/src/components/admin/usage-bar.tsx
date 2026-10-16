import { cn } from "@tessera/ui/lib/utils";

/** Past this line it is time to plan; past the next it is nearly full. */
export const WARN_PERCENT = 80;
export const CRITICAL_PERCENT = 95;

/**
 * Usage as a percentage of the limit, rounded to an integer.
 *
 * Not capped: the limit is approximate under concurrency, and an overshoot must show
 * as 103%, not stop at 100%.
 */
export function usagePercent(used: number, total: number): number {
  if (total <= 0) {
    return 0;
  }
  return Math.round((used / total) * 100);
}

/**
 * A usage bar. Do not draw one for something without a limit: a ratio of "unlimited"
 * means nothing.
 *
 * Decorative only: "used / limit" is always written beside it, so screen readers need
 * not hear it twice.
 */
export function UsageBar({ used, total }: { used: number; total: number }) {
  const percent = usagePercent(used, total);

  return (
    <div
      aria-hidden
      className="h-1.5 w-full overflow-hidden rounded-full bg-muted"
    >
      <div
        className={cn(
          "h-full rounded-full transition-[width]",
          percent >= CRITICAL_PERCENT && "bg-destructive",
          percent >= WARN_PERCENT &&
            percent < CRITICAL_PERCENT &&
            "bg-amber-500",
          percent < WARN_PERCENT && "bg-primary"
        )}
        style={{ width: `${Math.min(percent, 100)}%` }}
      />
    </div>
  );
}

/**
 * The first segment is opaque and each later one fainter, but never invisible.
 *
 * One hue (`--primary`): the design system's `--chart-*` is a grey scale and cannot
 * serve as categorical colors. Color only links a segment to the card that names it;
 * every card states its own numbers, so nothing depends on color.
 */
export function segmentOpacity(index: number): number {
  return Math.max(1 - index * 0.15, 0.4);
}

/**
 * Where content lives, as one bar split by location. Callers draw it only when
 * something is stored, since an empty split says nothing.
 *
 * Decorative only: every location states its usage elsewhere.
 */
export function SegmentedUsageBar({
  segments,
}: {
  segments: { name: string; usedBytes: number }[];
}) {
  const total = segments.reduce((sum, segment) => sum + segment.usedBytes, 0);

  return (
    <div
      aria-hidden
      className="flex h-2 w-full gap-px overflow-hidden rounded-full bg-muted"
    >
      {segments.map((segment, index) =>
        segment.usedBytes > 0 ? (
          <div
            className="h-full bg-primary transition-[width]"
            key={segment.name}
            style={{
              opacity: segmentOpacity(index),
              width: `${(segment.usedBytes / total) * 100}%`,
            }}
          />
        ) : null
      )}
    </div>
  );
}
