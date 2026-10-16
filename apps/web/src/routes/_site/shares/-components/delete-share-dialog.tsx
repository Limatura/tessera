import { Button } from "@tessera/ui/components/button";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";

import { useDeleteMyShares } from "@/hooks/use-shares";
import { settledErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

import type { Share } from "./share-fields";

/**
 * Confirmation before deleting your own shares, single or batch. A single share is
 * named by its code, since "My shares" has no other name for it; a batch gives a count.
 *
 * Stays open on partial failure: `onDeleted` tells the parent which ones went, `shares`
 * keeps only the rest, and pressing delete again retries them.
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
    <ResponsiveDialog
      alert
      onOpenChange={handleOpenChange}
      open={shares.length > 0}
    >
      <ResponsiveDialogContent>
        <ResponsiveDialogHeader>
          <ResponsiveDialogTitle>
            {shares.length === 1 && only
              ? m.shares_delete_dialog_title({ code: only.code })
              : m.shares_bulk_delete_title({ count: shares.length })}
          </ResponsiveDialogTitle>
          <ResponsiveDialogDescription>
            {m.shares_delete_dialog_description()}
          </ResponsiveDialogDescription>
        </ResponsiveDialogHeader>
        {failed.length > 0 && (
          <p className="text-destructive text-sm" role="alert">
            {settledErrorMessage(failed, m.shares_delete_dialog_error())}
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
