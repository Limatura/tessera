import { Button } from "@tessera/ui/components/button";
import { Field } from "@tessera/ui/components/field";

/** 每個設定表單底下那一塊：權限說明、server 的錯誤、送出按鈕。 */
export function SettingsFormFooter({
  canWrite,
  isDirty,
  isPending,
  isSaved,
  errorMessage,
}: {
  canWrite: boolean;
  isDirty: boolean;
  isPending: boolean;
  isSaved: boolean;
  errorMessage: string | null;
}) {
  return (
    <>
      {!canWrite && (
        <p className="text-muted-foreground text-sm">
          你只能檢視這些設定，變更需要 settings:write 權限。
        </p>
      )}

      {errorMessage && (
        <p className="text-destructive text-sm" role="alert">
          {errorMessage}
        </p>
      )}

      <Field orientation="horizontal">
        <Button disabled={!(canWrite && isDirty) || isPending} type="submit">
          {isPending ? "儲存中…" : "儲存"}
        </Button>
        {isSaved && (
          <span className="text-muted-foreground text-sm">已儲存</span>
        )}
      </Field>
    </>
  );
}
