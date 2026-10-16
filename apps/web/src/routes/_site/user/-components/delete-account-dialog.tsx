import { IconTrash } from "@tabler/icons-react";
import { useNavigate } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { useSession } from "@tessera/auth/hooks";
import { hasPermission, PERMISSIONS } from "@tessera/auth/permissions";
import { Button } from "@tessera/ui/components/button";
import { Checkbox } from "@tessera/ui/components/checkbox";
import {
  Field,
  FieldContent,
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
import { toast } from "@tessera/ui/components/toast";
import { useState } from "react";

import { SignInAgain } from "@/components/sign-in-again";
import { useDeleteAccount, useHasPassword } from "@/hooks/use-account";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

type ErrorBody = components["schemas"]["ErrorBody"];

function errorMessage(error: Partial<ErrorBody> | null): string | null {
  if (!error) {
    return null;
  }
  switch (error.code) {
    case "current_password_incorrect": {
      return m.user_profile_wrong_password();
    }
    // SignInAgain explains it; not repeated here.
    case "reauthentication_required": {
      return null;
    }
    // If the last holder of `roles:write` deletes themselves, no one can grant permissions.
    case "last_administrator": {
      return m.user_danger_delete_last_admin();
    }
    default: {
      return apiErrorMessage(error, m.common_try_again());
    }
  }
}

export function DeleteAccountButton() {
  const [open, setOpen] = useState(false);

  return (
    <>
      <Button
        onClick={() => setOpen(true)}
        size="sm"
        type="button"
        variant="destructive"
      >
        <IconTrash size={16} />
        {m.user_danger_delete_action()}
      </Button>
      <ResponsiveDialog alert onOpenChange={setOpen} open={open}>
        <ResponsiveDialogContent>
          {/* Unmounted on close, so the confirmation text, password and checkbox start fresh. */}
          {open ? <DeleteAccountForm onCancel={() => setOpen(false)} /> : null}
        </ResponsiveDialogContent>
      </ResponsiveDialog>
    </>
  );
}

function DeleteAccountForm({ onCancel }: { onCancel: () => void }) {
  const navigate = useNavigate();
  const deleteAccount = useDeleteAccount();
  const { data: session } = useSession();
  // The client cannot tell whether this is the last administrator, so it can
  // only warn; if it is, the backend answers `last_administrator`.
  const isAdministrator = hasPermission(session, PERMISSIONS.rolesWrite);
  const hasPassword = useHasPassword();
  /*
   * Deleting requires typing a confirmation phrase. It guards against slips, not
   * identity: that is the password or a recent sign-in, checked by the backend.
   * Pasting is allowed, since it stops accidents, not someone who means it.
   */
  const phrase = m.user_danger_delete_phrase();
  const [confirmText, setConfirmText] = useState("");
  const confirmed = confirmText.trim() === phrase;
  const [password, setPassword] = useState("");
  // Shares go too by default: "delete my account" usually covers what I shared
  // (see DeleteAccountRequest).
  const [deleteShares, setDeleteShares] = useState(true);

  const error = errorMessage(deleteAccount.error as Partial<ErrorBody> | null);
  const needsSignIn =
    (deleteAccount.error as Partial<ErrorBody> | null)?.code ===
    "reauthentication_required";
  const canSubmit =
    confirmed &&
    (!hasPassword || password.length > 0) &&
    !deleteAccount.isPending;

  const handleDelete = (event: React.FormEvent) => {
    event.preventDefault();
    deleteAccount.mutate(
      {
        current_password: hasPassword ? password : undefined,
        delete_shares: deleteShares,
      },
      {
        onSuccess: () => {
          toast.add({ title: m.user_danger_delete_done(), type: "success" });
          navigate({ to: "/" });
        },
      }
    );
  };

  return (
    <form className="contents" onSubmit={handleDelete}>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {m.user_danger_delete_confirm_title()}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {m.user_danger_delete_confirm_description()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>

      {isAdministrator ? (
        <p className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
          {m.user_danger_delete_admin_notice()}
        </p>
      ) : null}

      <div className="space-y-4">
        <Field data-invalid={!hasPassword && Boolean(error)}>
          <FieldLabel htmlFor="delete-account-confirm">
            {m.user_danger_delete_type_phrase({ phrase })}
          </FieldLabel>
          <Input
            autoComplete="off"
            // oxlint-disable-next-line jsx-a11y/no-autofocus -- first input in the dialog
            autoFocus
            id="delete-account-confirm"
            onChange={(event) => setConfirmText(event.target.value)}
            placeholder={phrase}
            spellCheck={false}
            value={confirmText}
          />
          {!hasPassword && error ? <FieldError>{error}</FieldError> : null}
        </Field>

        {hasPassword ? (
          <Field data-invalid={Boolean(error)}>
            <FieldLabel htmlFor="delete-account-password">
              {m.user_password_current()}
            </FieldLabel>
            <Input
              autoComplete="current-password"
              id="delete-account-password"
              onChange={(event) => setPassword(event.target.value)}
              type="password"
              value={password}
            />
            {error ? <FieldError>{error}</FieldError> : null}
          </Field>
        ) : null}

        {needsSignIn ? (
          <SignInAgain message={m.user_reauth_required()} />
        ) : null}

        <Field orientation="horizontal">
          <Checkbox
            checked={deleteShares}
            id="delete-account-shares"
            onCheckedChange={(next) => setDeleteShares(next === true)}
          />
          <FieldContent>
            <FieldLabel htmlFor="delete-account-shares">
              {m.user_danger_delete_shares()}
            </FieldLabel>
            <FieldDescription>
              {deleteShares
                ? m.user_danger_delete_shares_on()
                : m.user_danger_delete_shares_off()}
            </FieldDescription>
          </FieldContent>
        </Field>
      </div>

      <ResponsiveDialogFooter>
        <Button onClick={onCancel} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button disabled={!canSubmit} type="submit" variant="destructive">
          {m.user_danger_delete_action()}
        </Button>
      </ResponsiveDialogFooter>
    </form>
  );
}
