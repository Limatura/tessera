/**
 * `bandwidth.*` is stored in bytes per second; the panel shows megabits per
 * second, the unit a speed test reports.
 */

/** Bytes per second in one Mbit/s. */
const BYTES_PER_MBIT = 125_000;

/** `BandwidthConfig::MIN_BYTES_PER_SEC` on the server. */
export const MIN_BANDWIDTH_BYTES = 1024;

/** The share of a measured speed suggested as a limit. */
export const SUGGESTED_SHARE = 0.5;

/**
 * Six decimals is 0.125 B/s, finer than a whole byte, so a value set in config
 * comes back from `mbpsToBytes` as the same number of bytes and saving does not
 * change it.
 */
export function bytesToMbps(bytes: number): number {
  return Math.round((bytes / BYTES_PER_MBIT) * 1_000_000) / 1_000_000;
}

export function mbpsToBytes(mbps: number): number {
  return Math.round(mbps * BYTES_PER_MBIT);
}

/** Half the measured speed, to one decimal, never below the server's floor. */
export function suggestedMbps(measured: number): number {
  const half = Math.round(measured * SUGGESTED_SHARE * 10) / 10;
  return Math.max(half, bytesToMbps(MIN_BANDWIDTH_BYTES) + 0.01);
}
