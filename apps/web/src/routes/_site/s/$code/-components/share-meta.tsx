import { formatBytes } from "@tessera/ui/lib/format";

import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import * as m from "@/paraglide/messages";

/** 卡片標題下的一行：大小、剩幾次、何時過期。檔案、文字、連結三種卡片共用。 */
export function ShareMeta({
  expiresAt,
  sizeBytes,
  usesRemaining,
}: {
  expiresAt?: string | null;
  sizeBytes?: number | null;
  usesRemaining?: number | null;
}) {
  const formatDate = useDateTimeFormatter();
  const parts = [
    sizeBytes === null || sizeBytes === undefined
      ? null
      : formatBytes(sizeBytes),
    usesRemaining === null || usesRemaining === undefined
      ? m.share_uses_unlimited()
      : m.share_uses_remaining({ count: usesRemaining }),
    expiresAt
      ? m.share_expires_at({ date: formatDate(expiresAt) })
      : m.share_never_expires(),
  ].filter(Boolean);

  return <p className="text-muted-foreground text-xs">{parts.join(" · ")}</p>;
}
