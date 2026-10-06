import { IconTrash } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";

import * as m from "@/paraglide/messages";

import type { Share } from "./share-fields";

/** 表格跟手機列表共用的刪除按鈕;實際的確認與請求在 `DeleteShareDialog`。 */
export function DeleteShareButton({
  share,
  onDelete,
}: {
  share: Share;
  onDelete: (share: Share) => void;
}) {
  return (
    <Button
      aria-label={m.shares_delete_aria({ code: share.code })}
      onClick={() => onDelete(share)}
      size="icon"
      type="button"
      variant="ghost"
    >
      <IconTrash />
    </Button>
  );
}
