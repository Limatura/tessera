const SECONDS_PER_MINUTE = 60;
const SECONDS_PER_HOUR = 3600;

/**
 * `m:ss`, or `h:mm:ss` once `reference` reaches an hour. Pass the duration as
 * `reference` so the current time and the total have the same shape and the
 * readout does not change width as playback crosses an hour.
 */
export function formatTime(seconds: number, reference = seconds): string {
  const total = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const hours = Math.floor(total / SECONDS_PER_HOUR);
  const minutes = Math.floor((total % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE);
  const secs = String(total % SECONDS_PER_MINUTE).padStart(2, "0");
  const withHours = Number.isFinite(reference) && reference >= SECONDS_PER_HOUR;
  if (withHours || hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${secs}`;
  }
  return `${minutes}:${secs}`;
}
