import { IconAdjustmentsHorizontal, IconTrash } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";

import * as m from "@/paraglide/messages";

import { shareState } from "./share-fields";
import type { Share } from "./share-fields";

/**
 * 表格跟手機列表共用的那兩顆按鈕:改設定、刪除。實際的動作在 `ManageShareDialog`
 * 跟 `DeleteShareDialog`。
 *
 * 已失效的不給改設定:檔案在失效後的下一次清掃就刪了,延長期限或加次數 server 會回 410
 * (`share_content_removed`),打開對話框只會撞牆。刪除留著 —— 那是清掉這筆紀錄的方法。
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
      {shareState(share) === "live" && (
        <Button
          aria-label={m.shares_manage_aria({ code: share.code })}
          onClick={() => onManage(share)}
          size="icon"
          type="button"
          variant="ghost"
        >
          <IconAdjustmentsHorizontal />
        </Button>
      )}
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
