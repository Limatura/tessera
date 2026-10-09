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

import { useDeleteShare } from "@/hooks/admin/use-all-shares";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

type AdminShare = components["schemas"]["AdminShareResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];

export function DeleteShareDialog({
  share,
  label,
  onOpenChange,
}: {
  share: AdminShare | null;
  /** 表格上顯示的那個名字,對話框跟著用,才認得出是哪一份。 */
  label: string;
  onOpenChange: (open: boolean) => void;
}) {
  const deleteShare = useDeleteShare();
  const error = deleteShare.error as Partial<ErrorBody> | null;

  const handleOpenChange = (open: boolean) => {
    if (!open) {
      deleteShare.reset();
    }
    onOpenChange(open);
  };

  const handleConfirm = () => {
    if (share) {
      deleteShare.mutate(share.id, {
        onSuccess: () => handleOpenChange(false),
      });
    }
  };

  return (
    <AlertDialog onOpenChange={handleOpenChange} open={share !== null}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {m.all_shares_delete_dialog_title({ label })}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {m.all_shares_delete_dialog_description()}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {error && (
          <p className="text-destructive text-sm" role="alert">
            {apiErrorMessage(error, m.all_shares_delete_dialog_error())}
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
