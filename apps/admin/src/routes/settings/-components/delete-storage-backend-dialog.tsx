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

/** `409` 代表這列還在持有內容——移除前得先把東西搬走，錯誤訊息直接顯示 server 給的原因。 */
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
            {error.message ?? m.settings_storage_backends_delete_error()}
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
