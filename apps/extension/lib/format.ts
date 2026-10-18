import { i18n } from "#i18n";

const HOURS_PER_DAY = 24;

/** A lifetime in whole days where it is one, else in hours; `0` is forever. */
export function expiryLabel(hours: number): string {
  if (hours === 0) {
    return i18n.t("expiry.never");
  }
  return hours % HOURS_PER_DAY === 0
    ? i18n.t("expiry.days", hours / HOURS_PER_DAY)
    : i18n.t("expiry.hours", hours);
}
