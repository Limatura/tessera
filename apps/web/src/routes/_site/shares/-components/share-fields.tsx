import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";

import { isExpired } from "@/lib/expiry-label";
import * as m from "@/paraglide/messages";

export type Share = components["schemas"]["ShareResponse"];

/**
 * 分享還能不能取。`ShareResponse` 沒有 status 欄位,從期限跟剩餘次數推:
 * 兩者都會讓連結失效,而失效的分享檔案在下一次清掃就會被刪,紀錄再留一段時間。
 */
export type ShareState = "live" | "expired" | "exhausted";

export function shareState(share: Share): ShareState {
  if (isExpired(share.expires_at)) {
    return "expired";
  }
  if (share.uses_remaining === 0) {
    return "exhausted";
  }
  return "live";
}

/** 失效的原因。還能取的不顯示 —— 正常狀態不需要標籤。 */
export function ShareStateBadge({ state }: { state: ShareState }) {
  if (state === "live") {
    return null;
  }
  return (
    <Badge variant="outline">
      {state === "expired"
        ? m.all_shares_status_expired()
        : m.all_shares_status_exhausted()}
    </Badge>
  );
}

/** 表格跟手機列表共用的類型／密碼標籤,兩種版面顯示的東西要一致。 */
export function ShareBadges({ share }: { share: Share }) {
  return (
    <span className="flex flex-wrap gap-1">
      <Badge variant="secondary">{shareKindLabel(share.kind)}</Badge>
      {share.has_password && (
        <Badge variant="outline">{m.shares_needs_password()}</Badge>
      )}
    </span>
  );
}

/** API 的 `kind` 是英文代碼,顯示前翻成介面語言;未知值原樣顯示,後端加新類型時不會空白。 */
function shareKindLabel(kind: string): string {
  switch (kind) {
    case "text": {
      return m.shares_kind_text();
    }
    case "file": {
      return m.shares_kind_file();
    }
    case "link": {
      return m.shares_kind_link();
    }
    default: {
      return kind;
    }
  }
}
