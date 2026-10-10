import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@tessera/ui/components/alert-dialog";
import { Button } from "@tessera/ui/components/button";

import { useDeleteMyShares } from "@/hooks/use-shares";
import { settledErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

import type { Share } from "./share-fields";

/**
 * 刪除自己的分享前的確認,單筆跟批次共用。單筆以取件碼辨認是哪一份,
 * 因為「我的分享」沒有別的名字;批次只講份數。
 *
 * 部分失敗時不關:刪掉的那幾份由 `onDeleted` 通知外面拿掉,`shares` 只剩沒刪掉的,
 * 再按一次刪除就是重試那幾份。
 */
export function DeleteShareDialog({
  shares,
  onOpenChange,
  onDeleted,
}: {
  shares: Share[];
  onOpenChange: (open: boolean) => void;
  onDeleted: (codes: string[]) => void;
}) {
  const deleteShares = useDeleteMyShares();
  const failed = deleteShares.data?.failed ?? [];

  const handleOpenChange = (open: boolean) => {
    if (!open) {
      deleteShares.reset();
    }
    onOpenChange(open);
  };

  const handleConfirm = () => {
    deleteShares.mutate(
      shares.map((share) => share.code),
      {
        onSuccess: ({ done, failed: left }) => {
          onDeleted(done);
          if (left.length === 0) {
            handleOpenChange(false);
          }
        },
      }
    );
  };

  const [only] = shares;

  return (
    <AlertDialog onOpenChange={handleOpenChange} open={shares.length > 0}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {shares.length === 1 && only
              ? m.shares_delete_dialog_title({ code: only.code })
              : m.shares_bulk_delete_title({ count: shares.length })}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {m.shares_delete_dialog_description()}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {failed.length > 0 && (
          <p className="text-destructive text-sm" role="alert">
            {settledErrorMessage(failed, m.shares_delete_dialog_error())}
          </p>
        )}
        <AlertDialogFooter>
          <Button
            onClick={() => handleOpenChange(false)}
            type="button"
            variant="outline"
          >
            {m.common_cancel()}
          </Button>
          <Button
            disabled={deleteShares.isPending}
            onClick={handleConfirm}
            type="button"
            variant="destructive"
          >
            {deleteShares.isPending ? m.common_deleting() : m.common_delete()}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
