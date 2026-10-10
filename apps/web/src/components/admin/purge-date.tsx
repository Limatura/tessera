import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { AdminShare } from "@/lib/admin/admin-shares";
import * as m from "@/paraglide/messages";

/**
 * 失效的分享這一列什麼時候會被清掃刪掉,放在狀態標籤旁邊。
 * 回答「過期的怎麼還在」:不是清理壞了,是留到這天。還有效的(`purges_at` 是 `null`)不顯示。
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
