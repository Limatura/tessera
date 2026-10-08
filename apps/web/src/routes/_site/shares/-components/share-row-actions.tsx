import { IconAdjustmentsHorizontal, IconTrash } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";

import * as m from "@/paraglide/messages";

import type { Share } from "./share-fields";

/**
 * 表格跟手機列表共用的那兩顆按鈕:改設定、刪除。實際的動作在 `ManageShareDialog`
 * 跟 `DeleteShareDialog`。
 */
export function ShareRowActions({
  share,
  onManage,
  onDelete,
}: {
  share: Share;
  onManage: (share: Share) => void;
  onDelete: (share: Share) => void;
}) {
  return (
    <span className="flex justify-end gap-1">
      <Button
        aria-label={m.shares_manage_aria({ code: share.code })}
        onClick={() => onManage(share)}
        size="icon"
        type="button"
        variant="ghost"
      >
        <IconAdjustmentsHorizontal />
      </Button>
      <Button
        aria-label={m.shares_delete_aria({ code: share.code })}
        onClick={() => onDelete(share)}
        size="icon"
        type="button"
        variant="ghost"
      >
        <IconTrash />
      </Button>
    </span>
  );
}
