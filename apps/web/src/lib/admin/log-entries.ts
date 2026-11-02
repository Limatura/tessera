import type { components } from "@tessera/api/schema";

export type LogEntry = components["schemas"]["Entry"];
export type LogLevel = components["schemas"]["LogLevel"];

/**
 * The most lines the page keeps in memory, newest first. Past it the oldest are
 * dropped as new ones arrive, and reading further back stops: narrow the filter
 * or download the day instead.
 */
export const MAX_HELD_ENTRIES = 5000;

export const LOG_LEVELS = [
  "trace",
  "debug",
  "info",
  "warn",
  "error",
] as const satisfies readonly LogLevel[];

/**
 * An entry `id` is `YYYY-MM-DD:offset`: the day, then the byte offset in that
 * day's file. The offset is a number, so `…:9` sorts before `…:10`, which a plain
 * string compare gets wrong.
 */
export function compareEntryIds(a: string, b: string): number {
  const [dayA = "", offsetA = ""] = a.split(":");
  const [dayB = "", offsetB = ""] = b.split(":");
  if (dayA !== dayB) {
    return dayA < dayB ? -1 : 1;
  }
  return Number(offsetA) - Number(offsetB);
}

/**
 * Puts `incoming`, a batch from the stream oldest line first, in front of
 * `held`, which runs newest first. Lines already held are skipped, since a
 * reconnect refetches the newest page and overlaps what the stream sent. When
 * the result is over the cap, the oldest lines, at the end, go.
 */
export function mergeNewer(
  held: readonly LogEntry[],
  incoming: readonly LogEntry[]
): LogEntry[] {
  if (incoming.length === 0) {
    return trim(held);
  }
  const newestFirst = incoming.toReversed();
  const [newest] = held;
  const [oldestIncoming] = newestFirst.toReversed();
  // The usual case: a live batch that continues where the list begins.
  if (
    !newest ||
    (oldestIncoming && compareEntryIds(newest.id, oldestIncoming.id) < 0)
  ) {
    return trim([...newestFirst, ...held]);
  }
  const seen = new Set(held.map((entry) => entry.id));
  const fresh = newestFirst.filter((entry) => !seen.has(entry.id));
  return trim(
    [...fresh, ...held].toSorted((a, b) => compareEntryIds(b.id, a.id))
  );
}

/** Older lines, newest first as the server sends them, to put after `held`. */
export function appendOlder(
  held: readonly LogEntry[],
  older: readonly LogEntry[]
): LogEntry[] {
  const seen = new Set(held.map((entry) => entry.id));
  return [...held, ...older.filter((entry) => !seen.has(entry.id))];
}

/** An RFC 3339 time as the `YYYY-MM-DDTHH:mm` a `datetime-local` input wants, in local time. */
export function toLocalInput(iso: string | null): string {
  if (!iso) {
    return "";
  }
  const date = new Date(iso);
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, "YYYY-MM-DDTHH:mm".length);
}

/** The reverse of `toLocalInput`; a cleared input is `null`. */
export function fromLocalInput(value: string): string | null {
  return value ? new Date(value).toISOString() : null;
}

function trim(entries: readonly LogEntry[]): LogEntry[] {
  return entries.length > MAX_HELD_ENTRIES
    ? entries.slice(0, MAX_HELD_ENTRIES)
    : [...entries];
}
