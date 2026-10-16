import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { AdminShare } from "@/lib/admin/admin-shares";
import * as m from "@/paraglide/messages";

/**
 * When a dead share's row will be swept, shown next to its status badge. It
 * answers "why is this expired one still here": cleanup is not broken, the row
 * is kept until this date. Not shown for live shares (`purges_at` is `null`).
 */
export function PurgeDate({ share }: { share: AdminShare }) {
  const formatDate = useDateTimeFormatter();
  if (!share.purges_at) {
    return null;
  }
  return (
    <span className="text-muted-foreground text-xs tabular-nums">
      {m.all_shares_purges_at({ date: formatDate(share.purges_at) })}
    </span>
  );
}
