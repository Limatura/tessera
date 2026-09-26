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

import { useDeleteShare } from "@/hooks/use-admin";

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
          <AlertDialogTitle>刪除「{label}」？</AlertDialogTitle>
          <AlertDialogDescription>
            連結會立刻失效,已經拿到取件碼的人也打不開。檔案稍後由背景清理移除,無法復原。
          </AlertDialogDescription>
        </AlertDialogHeader>
        {error && (
          <p className="text-destructive text-sm" role="alert">
            {error.message ?? "刪除失敗,請再試一次"}
          </p>
        )}
        <AlertDialogFooter>
          <Button
            onClick={() => handleOpenChange(false)}
            type="button"
            variant="outline"
          >
            取消
          </Button>
          <Button
            disabled={deleteShare.isPending}
            onClick={handleConfirm}
            type="button"
            variant="destructive"
          >
            {deleteShare.isPending ? "刪除中…" : "刪除"}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
