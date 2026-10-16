/**
 * Unit menus for number settings: the form keeps the value in the unit the
 * server stores (seconds, hours, bytes, KiB...), the input shows it in whichever
 * unit the admin picks.
 *
 * Pure functions only, so the conversions are tested without a DOM.
 */

/** The unit a setting is stored in, as `NumberInput`'s `unit` prop names it. */
export type StoredUnit =
  | "seconds"
  | "minutes"
  | "hours"
  | "days"
  | "bytes"
  | "kib";

export type TimeUnit = "second" | "minute" | "hour" | "day";
export type SizeUnit = "KB" | "MB" | "GB" | "TB";
export type DisplayUnit = TimeUnit | SizeUnit;

const KIB = 1024;

/** Seconds or bytes per display unit. Sizes are 1024-based, like `formatBytes`. */
const DISPLAY_FACTOR: Record<DisplayUnit, number> = {
  GB: KIB ** 3,
  KB: KIB,
  MB: KIB ** 2,
  TB: KIB ** 4,
  day: 86_400,
  hour: 3600,
  minute: 60,
  second: 1,
};

/** Seconds or bytes per stored unit. */
const STORED_FACTOR: Record<StoredUnit, number> = {
  bytes: 1,
  days: 86_400,
  hours: 3600,
  kib: KIB,
  minutes: 60,
  seconds: 1,
};

const TIME_UNITS: TimeUnit[] = ["second", "minute", "hour", "day"];
/**
 * KB is offered too: most byte settings are upload or quota sizes, but a few
 * (the inline-display threshold, 64 KiB by default) live below a megabyte.
 */
const BYTE_UNITS: SizeUnit[] = ["KB", "MB", "GB", "TB"];
const KIB_UNITS: SizeUnit[] = ["KB", "MB", "GB"];

export function isSizeUnit(unit: DisplayUnit): unit is SizeUnit {
  return unit === "KB" || unit === "MB" || unit === "GB" || unit === "TB";
}

/**
 * The units a stored unit can be shown in, smallest first. Nothing finer than
 * what is stored: an `hours` setting cannot hold 30 minutes.
 */
export function unitsFor(stored: StoredUnit): DisplayUnit[] {
  if (stored === "bytes") {
    return BYTE_UNITS;
  }
  if (stored === "kib") {
    return KIB_UNITS;
  }
  const base = STORED_FACTOR[stored];
  return TIME_UNITS.filter((unit) => DISPLAY_FACTOR[unit] >= base);
}

/**
 * The unit to show a value in when the input appears: the largest one it is a
 * whole number of (900 seconds → 15 minutes, 2 GiB → 2 GB), otherwise the
 * smallest, so nothing shows as an awkward fraction.
 */
export function pickUnit(
  value: number | undefined,
  stored: StoredUnit
): DisplayUnit {
  const units = unitsFor(stored);
  const [smallest] = units;
  if (value === undefined || Number.isNaN(value) || value <= 0) {
    return smallest;
  }
  const amount = value * STORED_FACTOR[stored];
  return (
    units.findLast((unit) => amount % DISPLAY_FACTOR[unit] === 0) ?? smallest
  );
}

/** Stored value → the number shown in `unit`, to at most four decimals. */
export function toDisplay(
  value: number,
  stored: StoredUnit,
  unit: DisplayUnit
): number {
  const amount = (value * STORED_FACTOR[stored]) / DISPLAY_FACTOR[unit];
  return Number(amount.toFixed(4));
}

/**
 * A typed number in `unit` → the stored value, rounded to a whole stored unit:
 * the server takes integers, so 1.5 GB becomes a whole number of bytes and
 * 0.5 days twelve hours.
 */
export function fromDisplay(
  amount: number,
  stored: StoredUnit,
  unit: DisplayUnit
): number {
  return Math.round((amount * DISPLAY_FACTOR[unit]) / STORED_FACTOR[stored]);
}
