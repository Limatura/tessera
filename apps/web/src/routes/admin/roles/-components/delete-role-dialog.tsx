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
    <ResponsiveDialog alert onOpenChange={onOpenChange} open={open}>
      <ResponsiveDialogContent>
        <ResponsiveDialogHeader>
          <ResponsiveDialogTitle>
            {m.roles_delete_dialog_title({ name: roleLabel(role.name) })}
          </ResponsiveDialogTitle>
          <ResponsiveDialogDescription>
            {role.members > 0
              ? m.roles_delete_dialog_description_with_members({
                  members: role.members,
                })
              : m.roles_delete_dialog_description_empty()}
          </ResponsiveDialogDescription>
        </ResponsiveDialogHeader>
        {error && (
          <p className="text-destructive text-sm" role="alert">
            {apiErrorMessage(error, m.roles_delete_dialog_error())}
          </p>
        )}
        <ResponsiveDialogFooter>
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
        </ResponsiveDialogFooter>
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}
