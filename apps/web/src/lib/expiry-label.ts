import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/** Whether the expiry has passed, to colour the label. */
export function isExpired(expiresAt: string | null | undefined): boolean {
  return !!expiresAt && new Date(expiresAt).getTime() <= Date.now();
}

/**
 * A relative phrase like "expires in 3 days". The list is a snapshot, checked
 * against the server only when management opens.
 */
export function expiryLabel(expiresAt: string | null | undefined): string {
  if (!expiresAt) {
    return m.recent_shares_never_expires();
  }
  const remaining = new Date(expiresAt).getTime() - Date.now();
  if (remaining <= 0) {
    return m.recent_shares_expired();
  }
  const format = new Intl.RelativeTimeFormat(getLocale(), { numeric: "auto" });
  if (remaining >= DAY_MS) {
    return m.recent_shares_expires({
      when: format.format(Math.round(remaining / DAY_MS), "day"),
    });
  }
  if (remaining >= HOUR_MS) {
    return m.recent_shares_expires({
      when: format.format(Math.round(remaining / HOUR_MS), "hour"),
    });
  }
  return m.recent_shares_expires({
    when: format.format(
      Math.max(1, Math.round(remaining / MINUTE_MS)),
      "minute"
    ),
  });
}
