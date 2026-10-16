const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;
const BYTES_PER_UNIT = 1024;

/**
 * File size. Whole bytes have no decimals (`999 B`); larger units always have
 * one (`1.5 KB`).
 *
 * Negative values and 0 give `0 B`: `Math.log` of a negative is NaN, and a
 * negative file size can only be an upstream bug, where 0 is more useful than
 * NaN.
 */
export function formatBytes(bytes: number): string {
  const { value, unit, fractionDigits } = splitBytes(bytes);
  return `${value.toFixed(fractionDigits)} ${unit}`;
}

/**
 * `formatBytes` split into number and unit, for places that animate the number
 * while the unit stays put. Same rules as `formatBytes`: no decimals for whole
 * bytes, one decimal for larger units.
 */
export function splitBytes(bytes: number): {
  value: number;
  unit: (typeof BYTE_UNITS)[number];
  fractionDigits: 0 | 1;
} {
  if (bytes <= 0) {
    return { fractionDigits: 0, unit: "B", value: 0 };
  }

  const exponent = Math.min(
    Math.floor(Math.log(bytes) / Math.log(BYTES_PER_UNIT)),
    BYTE_UNITS.length - 1
  );

  return {
    fractionDigits: exponent === 0 ? 0 : 1,
    // `exponent` is already clamped to 0..length-1; `??` only narrows the type
    // for noUncheckedIndexedAccess.
    unit: BYTE_UNITS[exponent] ?? "B",
    value: bytes / BYTES_PER_UNIT ** exponent,
  };
}

/**
 * Thousands separators.
 *
 * `locale` is required: hardcoding any one language would show the other
 * interface languages inconsistent number formats. Callers always have
 * `getLocale()` at hand.
 */
export function formatNumber(value: number, locale: string): string {
  return new Intl.NumberFormat(locale).format(value);
}

/**
 * Date and time formatter following the interface language and the time format
 * preference.
 *
 * `auto` leaves `hour12` unset so `Intl` decides by locale. That differs
 * from 12-hour. Hardcoding `hour12: false` would show zh-TW users times
 * formatted unlike the rest of their system.
 */
export function createDateTimeFormatter(
  locale: string,
  timeFormat: "auto" | "12h" | "24h"
) {
  const formatter = new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    hour12: timeFormat === "auto" ? undefined : timeFormat === "12h",
    timeStyle: "short",
  });

  return (value: string | null | undefined) =>
    value ? formatter.format(new Date(value)) : "—";
}
