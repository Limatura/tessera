import { Button } from "@tessera/ui/components/button";
import { FieldGroup } from "@tessera/ui/components/field";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { useState } from "react";

import {
  ByteLimitField,
  modeOf,
  toBytes,
  toMib,
} from "@/components/admin/byte-limit-field";
import type { ByteLimitMode } from "@/components/admin/byte-limit-field";
import { useSetUserLimits } from "@/hooks/admin/use-users";
import type { AdminUser } from "@/lib/admin/accounts";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

/**
 * Set quotas for a single account without creating a role for one person.
 *
 * Both fields offer "use the role's" (`null`) and "unlimited" (`0`), which are
 * different: the first follows the role when it changes, the second exempts this
 * account from the role's limit for good. The site-wide
 * `storage.max_total_bytes` and `server.max_body_bytes` still apply on top;
 * nothing here grants more than the site allows.
 */
export function UserQuotaDialog({
  account,
  open,
  onOpenChange,
}: {
  account: AdminUser | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const close = () => onOpenChange(false);

  return (
    <ResponsiveDialog onOpenChange={onOpenChange} open={open}>
      <ResponsiveDialogContent className="sm:max-w-lg">
        {/* Keyed so switching accounts remounts with new initial values. */}
        {open && account && (
          <UserQuotaForm account={account} key={account.id} onDone={close} />
        )}
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}

function UserQuotaForm({
  account,
  onDone,
}: {
  account: AdminUser;
  onDone: () => void;
}) {
  const setLimits = useSetUserLimits();

  const initialQuotaMode = modeOf(account.quota_bytes);
  const initialQuotaMib = toMib(account.quota_bytes);
  const [quotaMode, setQuotaMode] = useState<ByteLimitMode>(initialQuotaMode);
  const [quotaMib, setQuotaMib] = useState(initialQuotaMib);

  const initialFileMode = modeOf(account.max_file_bytes);
  const initialFileMib = toMib(account.max_file_bytes);
  const [fileMode, setFileMode] = useState<ByteLimitMode>(initialFileMode);
  const [fileMib, setFileMib] = useState(initialFileMib);

  const quotaChanged =
    quotaMode !== initialQuotaMode ||
    (quotaMode === "custom" && quotaMib !== initialQuotaMib);
  const fileChanged =
    fileMode !== initialFileMode ||
    (fileMode === "custom" && fileMib !== initialFileMib);
  const invalid =
    (quotaMode === "custom" && !(quotaMib > 0)) ||
    (fileMode === "custom" && !(fileMib > 0));

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    const limits: {
      quota_bytes?: number | null;
      max_file_bytes?: number | null;
    } = {};
    if (quotaChanged) {
      limits.quota_bytes = toBytes(quotaMode, quotaMib);
    }
    if (fileChanged) {
      limits.max_file_bytes = toBytes(fileMode, fileMib);
    }
    setLimits.mutate({ id: account.id, limits }, { onSuccess: onDone });
  };

  return (
    <>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {m.users_quota_dialog_title({ name: account.username })}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {m.users_quota_dialog_description()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>

      <form id="user-quota-form" onSubmit={handleSubmit}>
        <FieldGroup>
          <ByteLimitField
            disabled={setLimits.isPending}
            id="user-quota"
            inheritDescription={m.users_quota_inherit_description()}
            inheritLabel={m.users_quota_inherit()}
            label={m.users_quota_label()}
            mib={quotaMib}
            mode={quotaMode}
            onMibChange={setQuotaMib}
            onModeChange={setQuotaMode}
          />
          <ByteLimitField
            disabled={setLimits.isPending}
            id="user-max-file"
            inheritDescription={m.users_quota_inherit_description()}
            inheritLabel={m.users_quota_inherit()}
            label={m.users_quota_file_label()}
            mib={fileMib}
            mode={fileMode}
            onMibChange={setFileMib}
            onModeChange={setFileMode}
          />
        </FieldGroup>

        {setLimits.error && (
          <p className="mt-3 text-destructive text-sm" role="alert">
            {apiErrorMessage(setLimits.error, m.users_quota_dialog_error())}
          </p>
        )}
      </form>

      <ResponsiveDialogFooter>
        <Button onClick={onDone} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={
            setLimits.isPending || invalid || !(quotaChanged || fileChanged)
          }
          form="user-quota-form"
          type="submit"
        >
          {setLimits.isPending ? m.common_saving() : m.common_save()}
        </Button>
      </ResponsiveDialogFooter>
    </>
  );
}
