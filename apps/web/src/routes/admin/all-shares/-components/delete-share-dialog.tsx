import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@tessera/ui/components/alert-dialog";
import { Button } from "@tessera/ui/components/button";

import { useDeleteShares } from "@/hooks/admin/use-all-shares";
import { shareLabel } from "@/lib/admin/admin-shares";
import type { AdminShare } from "@/lib/admin/admin-shares";
import { settledErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

/**
 * 單筆跟批次共用。單筆用表格上顯示的那個名字,才認得出是哪一份;批次只講份數。
 * 部分失敗時不關,`onDeleted` 通知外面拿掉刪掉的,剩下的就是要重試的。
 */
export function DeleteShareDialog({
  shares,
  onOpenChange,
  onDeleted,
}: {
  shares: AdminShare[];
  onOpenChange: (open: boolean) => void;
  onDeleted: (ids: string[]) => void;
}) {
  const deleteShares = useDeleteShares();
  const failed = deleteShares.data?.failed ?? [];

  const handleOpenChange = (open: boolean) => {
    if (!open) {
      deleteShares.reset();
    }
    onOpenChange(open);
  };

  const handleConfirm = () => {
    deleteShares.mutate(
      shares.map((share) => share.id),
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
              ? m.all_shares_delete_dialog_title({ label: shareLabel(only) })
              : m.shares_bulk_delete_title({ count: shares.length })}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {m.all_shares_delete_dialog_description()}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {failed.length > 0 && (
          <p className="text-destructive text-sm" role="alert">
            {settledErrorMessage(failed, m.all_shares_delete_dialog_error())}
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
