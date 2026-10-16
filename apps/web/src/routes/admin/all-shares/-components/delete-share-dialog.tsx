import { Button } from "@tessera/ui/components/button";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";

import { useDeleteShares } from "@/hooks/admin/use-all-shares";
import { shareLabel } from "@/lib/admin/admin-shares";
import type { AdminShare } from "@/lib/admin/admin-shares";
import { settledErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

/**
 * Single and batch alike. A single share uses the name shown in the table, so it can
 * be recognised; a batch gives a count. Stays open on partial failure: `onDeleted`
 * tells the parent which ones went, and what remains is what to retry.
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
    <ResponsiveDialog
      alert
      onOpenChange={handleOpenChange}
      open={shares.length > 0}
    >
      <ResponsiveDialogContent>
        <ResponsiveDialogHeader>
          <ResponsiveDialogTitle>
            {shares.length === 1 && only
              ? m.all_shares_delete_dialog_title({ label: shareLabel(only) })
              : m.shares_bulk_delete_title({ count: shares.length })}
          </ResponsiveDialogTitle>
          <ResponsiveDialogDescription>
            {m.all_shares_delete_dialog_description()}
          </ResponsiveDialogDescription>
        </ResponsiveDialogHeader>
        {failed.length > 0 && (
          <p className="text-destructive text-sm" role="alert">
            {settledErrorMessage(failed, m.all_shares_delete_dialog_error())}
          </p>
        )}
        <ResponsiveDialogFooter>
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
        </ResponsiveDialogFooter>
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}
