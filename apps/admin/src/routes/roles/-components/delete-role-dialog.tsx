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

import { useDeleteRole } from "@/hooks/use-admin";
import { roleLabel } from "@/lib/permissions";

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
          <AlertDialogTitle>刪除「{roleLabel(role.name)}」？</AlertDialogTitle>
          <AlertDialogDescription>
            {role.members > 0
              ? `目前有 ${role.members} 個帳號在這個角色裡。刪掉之後他們會變成沒有角色 —— 登入得了，但什麼都不能做。`
              : "這個角色目前沒有任何成員。"}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {error && (
          <p className="text-destructive text-sm" role="alert">
            {error.message ?? "刪除失敗"}
          </p>
        )}
        <AlertDialogFooter>
          <Button
            onClick={() => onOpenChange(false)}
            type="button"
            variant="outline"
          >
            取消
          </Button>
          <Button
            disabled={deleteRole.isPending}
            onClick={onConfirm}
            type="button"
            variant="destructive"
          >
            {deleteRole.isPending ? "刪除中…" : "刪除"}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
