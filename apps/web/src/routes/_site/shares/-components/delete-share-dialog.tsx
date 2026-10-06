import type { components } from "@tessera/api/schema";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@tessera/ui/components/alert-dialog";
import { Button } from "@tessera/ui/components/button";

import { useDeleteMyShare } from "@/hooks/use-shares";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

import type { Share } from "./share-fields";

type ErrorBody = components["schemas"]["ErrorBody"];

/** 刪除自己的分享前的確認。以取件碼辨認是哪一份,因為「我的分享」沒有別的名字。 */
export function DeleteShareDialog({
  share,
  onOpenChange,
}: {
  share: Share | null;
  onOpenChange: (open: boolean) => void;
}) {
  const deleteShare = useDeleteMyShare();
  const error = deleteShare.error as Partial<ErrorBody> | null;

  const handleOpenChange = (open: boolean) => {
    if (!open) {
      deleteShare.reset();
    }
    onOpenChange(open);
  };

  const handleConfirm = () => {
    if (share) {
      deleteShare.mutate(share.code, {
        onSuccess: () => handleOpenChange(false),
      });
    }
  };

  return (
    <AlertDialog onOpenChange={handleOpenChange} open={share !== null}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {m.shares_delete_dialog_title({ code: share?.code ?? "" })}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {m.shares_delete_dialog_description()}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {error && (
          <p className="text-destructive text-sm" role="alert">
            {apiErrorMessage(error, m.shares_delete_dialog_error())}
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
            disabled={deleteShare.isPending}
            onClick={handleConfirm}
            type="button"
            variant="destructive"
          >
            {deleteShare.isPending ? m.common_deleting() : m.common_delete()}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
