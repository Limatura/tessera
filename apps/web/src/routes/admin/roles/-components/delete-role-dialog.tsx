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

import { useDeleteRole } from "@/hooks/admin/use-roles";
import { roleLabel } from "@/lib/admin/permissions";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

type Role = components["schemas"]["RoleResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];

export function DeleteRoleDialog({
  role,
  open,
  onOpenChange,
}: {
  role: Role | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const deleteRole = useDeleteRole();
  const error = deleteRole.error as ErrorBody | null;

  if (!role) {
    return null;
  }

  const onConfirm = () => {
    deleteRole.mutate(role.id, { onSuccess: () => onOpenChange(false) });
  };

  return (
    <AlertDialog onOpenChange={onOpenChange} open={open}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {m.roles_delete_dialog_title({ name: roleLabel(role.name) })}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {role.members > 0
              ? m.roles_delete_dialog_description_with_members({
                  members: role.members,
                })
              : m.roles_delete_dialog_description_empty()}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {error && (
          <p className="text-destructive text-sm" role="alert">
            {apiErrorMessage(error, m.roles_delete_dialog_error())}
          </p>
        )}
        <AlertDialogFooter>
          <Button
            onClick={() => onOpenChange(false)}
            type="button"
            variant="outline"
          >
            {m.common_cancel()}
          </Button>
          <Button
            disabled={deleteRole.isPending}
            onClick={onConfirm}
            type="button"
            variant="destructive"
          >
            {deleteRole.isPending ? m.common_deleting() : m.common_delete()}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
