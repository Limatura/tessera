import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";

import { useRevokeInvite } from "@/hooks/admin/use-invites";
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
  /** The name shown in the table, reused in the dialog so the invite can be recognised. */
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
    <ResponsiveDialog
      alert
      onOpenChange={handleOpenChange}
      open={invite !== null}
    >
      <ResponsiveDialogContent>
        <ResponsiveDialogHeader>
          <ResponsiveDialogTitle>
            {m.invites_revoke_dialog_title({ label })}
          </ResponsiveDialogTitle>
          <ResponsiveDialogDescription>
            {m.invites_revoke_dialog_description()}
          </ResponsiveDialogDescription>
        </ResponsiveDialogHeader>
        {error && (
          <p className="text-destructive text-sm" role="alert">
            {apiErrorMessage(error, m.invites_revoke_dialog_error())}
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
            disabled={revokeInvite.isPending}
            onClick={handleConfirm}
            type="button"
            variant="destructive"
          >
            {revokeInvite.isPending
              ? m.invites_revoke_pending()
              : m.invites_revoke()}
          </Button>
        </ResponsiveDialogFooter>
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}
