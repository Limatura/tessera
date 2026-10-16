import type { components } from "@tessera/api/schema";

type StatsDay = components["schemas"]["StatsDay"];

/**
 * One day on the chart. `null` means no record that day (the server was not running),
 * drawn as a gap instead of a line across it.
 */
export interface TrendPoint {
  date: string;
  storedBytes: number | null;
  shares: number | null;
  liveShares: number | null;
  bytesServed: number | null;
}

export type TrendKey = Exclude<keyof TrendPoint, "date">;

const MS_PER_DAY = 86_400_000;

/**
 * Spreads the server's days into a continuous run of days.
 *
 * The server returns only days with a record; a day it was down is simply missing.
 * Charted as is, the days on either side would sit next to each other with a line
 * straight across, as if nothing changed, so missing days become `null` and break the
 * line.
 *
 * Nothing is filled before the first record: history starts on the day this version
 * was deployed, and choosing 90 days with 3 days of data would push the real data into
 * a corner behind 87 empty days.
 */
export function buildTrendPoints(days: StatsDay[]): {
  points: TrendPoint[];
  hasGaps: boolean;
} {
  const first = days.at(0);
  const last = days.at(-1);
  if (!first || !last) {
    return { hasGaps: false, points: [] };
  }

  const byDate = new Map(days.map((day) => [day.date, day]));
  const points: TrendPoint[] = [];
  const end = dayNumber(last.date);
  for (let current = dayNumber(first.date); current <= end; current += 1) {
    const date = new Date(current * MS_PER_DAY)
      .toISOString()
      .slice(0, "YYYY-MM-DD".length);
    const day = byDate.get(date);
    points.push({
      bytesServed: day?.bytes_served ?? null,
      date,
      liveShares: day?.live_shares ?? null,
      shares: day?.shares ?? null,
      storedBytes: day?.stored_bytes ?? null,
    });
  }

  return { hasGaps: points.length > days.length, points };
}

/**
 * Change from the first to the last point with a value, and the days between them.
 * Fewer than two points have no change to report.
 */
export function trendChange(
  points: TrendPoint[],
  key: TrendKey
): { delta: number; days: number } | null {
  const recorded = points.filter((point) => point[key] !== null);
  const first = recorded.at(0);
  const last = recorded.at(-1);
  if (!first || !last || first === last) {
    return null;
  }
  return {
    days: dayNumber(last.date) - dayNumber(first.date),
    delta: (last[key] ?? 0) - (first[key] ?? 0),
  };
}

/** `YYYY-MM-DD` (UTC) to days since the epoch, for stepping back day by day and measuring gaps. */
function dayNumber(date: string): number {
  return Math.round(Date.parse(`${date}T00:00:00Z`) / MS_PER_DAY);
}
