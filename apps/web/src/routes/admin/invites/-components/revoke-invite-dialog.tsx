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

import { useRevokeInvite } from "@/hooks/admin/use-admin";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

type Invite = components["schemas"]["InviteResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];

export function RevokeInviteDialog({
  invite,
  label,
  onOpenChange,
}: {
  invite: Invite | null;
  /** 表格上顯示的那個名字，對話框跟著用，才認得出是哪一組。 */
  label: string;
  onOpenChange: (open: boolean) => void;
}) {
  const revokeInvite = useRevokeInvite();
  const error = revokeInvite.error as Partial<ErrorBody> | null;

  const handleOpenChange = (open: boolean) => {
    if (!open) {
      revokeInvite.reset();
    }
    onOpenChange(open);
  };

  const handleConfirm = () => {
    if (invite) {
      revokeInvite.mutate(invite.id, {
        onSuccess: () => handleOpenChange(false),
      });
    }
  };

  return (
    <AlertDialog onOpenChange={handleOpenChange} open={invite !== null}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {m.invites_revoke_dialog_title({ label })}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {m.invites_revoke_dialog_description()}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {error && (
          <p className="text-destructive text-sm" role="alert">
            {apiErrorMessage(error, m.invites_revoke_dialog_error())}
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
            disabled={revokeInvite.isPending}
            onClick={handleConfirm}
            type="button"
            variant="destructive"
          >
            {revokeInvite.isPending
              ? m.invites_revoke_pending()
              : m.invites_revoke()}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
