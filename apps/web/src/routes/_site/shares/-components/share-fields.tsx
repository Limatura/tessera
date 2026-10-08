import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";

import * as m from "@/paraglide/messages";

export type Share = components["schemas"]["ShareResponse"];

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
