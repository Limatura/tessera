import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { useState } from "react";
import type * as React from "react";

import { SignInAgain } from "@/components/sign-in-again";
import { useRemoveStorageBackend } from "@/hooks/admin/use-storage-backends";
import { useHasPassword } from "@/hooks/use-account";
import { useMfaStatus } from "@/hooks/use-mfa";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

type Backend = components["schemas"]["BackendResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];

/**
 * The server only removes a backend that holds no files, answering
 * `409 backend_in_use` otherwise, so removal can never make content unreadable. Nor
 * does it delete anything in the bucket or folder: only Tessera's record of it goes.
 * The description states both.
 *
 * Removal also asks for the owner's password, or an authenticator code when two-step
 * sign-in is on. An account with no password confirms by a recent sign-in instead.
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
  return (
    <ResponsiveDialog alert onOpenChange={onOpenChange} open={open}>
      <ResponsiveDialogContent>
        {/* Unmounted on close, so the password or code does not outlive the dialog. */}
        {open && backend && (
          <DeleteForm
            backend={backend}
            onCancel={() => onOpenChange(false)}
            onDone={() => onOpenChange(false)}
          />
        )}
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}

function DeleteForm({
  backend,
  onCancel,
  onDone,
}: {
  backend: Backend;
  onCancel: () => void;
  onDone: () => void;
}) {
  const removeBackend = useRemoveStorageBackend();
  const hasPassword = useHasPassword();
  const { data: mfa } = useMfaStatus();
  const canUseCode = mfa?.totp_enabled === true;
  const [useCode, setUseCode] = useState(false);
  const [secret, setSecret] = useState("");

  const error = removeBackend.error as ErrorBody | null;
  const needsSignIn = error?.code === "reauthentication_required";
  const asksForSecret = useCode || hasPassword;
  const canSubmit =
    !backend.is_default &&
    (!asksForSecret || secret.length > 0) &&
    !removeBackend.isPending;

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    let confirm: { current_password?: string; code?: string } = {};
    if (useCode) {
      confirm = { code: secret.trim() };
    } else if (hasPassword) {
      confirm = { current_password: secret };
    }
    removeBackend.mutate(
      { confirm, name: backend.name },
      { onSuccess: onDone }
    );
  };

  const toggleMethod = () => {
    setUseCode((previous) => !previous);
    setSecret("");
    removeBackend.reset();
  };

  return (
    <form className="contents" onSubmit={handleSubmit}>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {m.settings_storage_backends_delete_title({ name: backend.name })}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {backend.is_default
            ? m.settings_storage_backends_delete_description_default()
            : `${m.settings_storage_backends_delete_description()} ${m.settings_storage_backends_delete_files_note()}`}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>

      {!backend.is_default && asksForSecret && (
        <ConfirmSecretField
          canSwitch={canUseCode && hasPassword}
          error={needsSignIn ? null : error}
          onChange={setSecret}
          onSwitch={toggleMethod}
          useCode={useCode}
          value={secret}
        />
      )}

      {needsSignIn && <SignInAgain message={m.user_reauth_required()} />}
      {error && !asksForSecret && !needsSignIn && (
        <p className="text-destructive text-sm" role="alert">
          {deleteErrorMessage(error)}
        </p>
      )}

      <ResponsiveDialogFooter>
        <Button onClick={onCancel} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button disabled={!canSubmit} type="submit" variant="destructive">
          {removeBackend.isPending ? m.common_deleting() : m.common_delete()}
        </Button>
      </ResponsiveDialogFooter>
    </form>
  );
}

/** The password, or the authenticator code when `useCode`, with the switch between them. */
function ConfirmSecretField({
  useCode,
  value,
  error,
  canSwitch,
  onChange,
  onSwitch,
}: {
  useCode: boolean;
  value: string;
  error: ErrorBody | null;
  canSwitch: boolean;
  onChange: (value: string) => void;
  onSwitch: () => void;
}) {
  return (
    <Field data-invalid={error !== null}>
      <FieldLabel htmlFor="delete-backend-secret">
        {useCode ? m.user_mfa_code_or_recovery() : m.user_password_current()}
      </FieldLabel>
      <Input
        autoComplete={useCode ? "one-time-code" : "current-password"}
        // oxlint-disable-next-line jsx-a11y/no-autofocus -- the only input in the dialog
        autoFocus
        id="delete-backend-secret"
        onChange={(event) => onChange(event.target.value)}
        type={useCode ? "text" : "password"}
        value={value}
      />
      <FieldDescription>
        {m.settings_storage_backends_delete_confirm_hint()}
      </FieldDescription>
      {error && <FieldError>{deleteErrorMessage(error)}</FieldError>}
      {canSwitch && (
        <button
          className="self-start text-muted-foreground text-xs underline underline-offset-4 hover:text-foreground"
          onClick={onSwitch}
          type="button"
        >
          {useCode
            ? m.settings_storage_backends_delete_use_password()
            : m.settings_storage_backends_delete_use_code()}
        </button>
      )}
    </Field>
  );
}

/**
 * `backend_in_use` gets a hint about the next step here; other errors use the
 * usual translation.
 */
function deleteErrorMessage(error: ErrorBody): string {
  if (error.code === "backend_in_use") {
    return m.settings_storage_backends_delete_in_use();
  }
  if (error.code === "current_password_incorrect") {
    return m.user_profile_wrong_password();
  }
  return apiErrorMessage(error, m.settings_storage_backends_delete_error());
}
