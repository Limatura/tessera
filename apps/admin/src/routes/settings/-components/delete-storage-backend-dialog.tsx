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

import { useRemoveStorageBackend } from "@/hooks/use-admin";
import * as m from "@/paraglide/messages";

type Backend = components["schemas"]["BackendResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];

/**
 * 後端只允許移除沒有檔案的 backend，還有檔案時回 `409 backend_in_use`，
 * 不會發生「移除後內容讀不到」的情況，所以說明文字講的是這條規則。
 */
export function DeleteStorageBackendDialog({
  backend,
  open,
  onOpenChange,
}: {
  backend: Backend | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const removeBackend = useRemoveStorageBackend();
  const error = removeBackend.error as ErrorBody | null;

  if (!backend) {
    return null;
  }

  const onConfirm = () => {
    removeBackend.mutate(backend.name, {
      onSuccess: () => onOpenChange(false),
    });
  };

  return (
    <AlertDialog onOpenChange={onOpenChange} open={open}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {m.settings_storage_backends_delete_title({ name: backend.name })}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {backend.is_default
              ? m.settings_storage_backends_delete_description_default()
              : m.settings_storage_backends_delete_description()}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {error && (
          <p className="text-destructive text-sm" role="alert">
            {deleteErrorMessage(error)}
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
            disabled={removeBackend.isPending || backend.is_default}
            onClick={onConfirm}
            type="button"
            variant="destructive"
          >
            {removeBackend.isPending ? m.common_deleting() : m.common_delete()}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/** `backend_in_use` 給翻譯好的原因和下一步，其他錯誤沿用 server 的訊息。 */
function deleteErrorMessage(error: ErrorBody): string {
  if (error.code === "backend_in_use") {
    return m.settings_storage_backends_delete_in_use();
  }
  return error.message ?? m.settings_storage_backends_delete_error();
}
