import { Button } from "@tessera/ui/components/button";
import { Field } from "@tessera/ui/components/field";

import * as m from "@/paraglide/messages";

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
          {m.settings_form_footer_read_only()}
        </p>
      )}

      {errorMessage && (
        <p className="text-destructive text-sm" role="alert">
          {errorMessage}
        </p>
      )}

      <Field orientation="horizontal">
        <Button disabled={!(canWrite && isDirty) || isPending} type="submit">
          {isPending
            ? m.settings_form_footer_saving()
            : m.settings_form_footer_save()}
        </Button>
        {isSaved && (
          <span className="text-muted-foreground text-sm">
            {m.settings_form_footer_saved()}
          </span>
        )}
      </Field>
    </>
  );
}
