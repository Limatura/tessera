import {
  IconDeviceMobile,
  IconKey,
  IconPencil,
  IconPlus,
  IconShieldLock,
  IconTrash,
} from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import { Field, FieldError, FieldLabel } from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { toast } from "@tessera/ui/components/toast";
import { useState } from "react";

import { SignInAgain } from "@/components/sign-in-again";
import { useHasPassword } from "@/hooks/use-account";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { Passkey } from "@/hooks/use-passkeys";
import {
  useAddPasskey,
  usePasskeys,
  useRemovePasskey,
  useRenamePasskey,
} from "@/hooks/use-passkeys";
import { apiErrorMessage } from "@/lib/api-error";
import { isPasskeyCancelled, isPasskeySupported } from "@/lib/webauthn";
import * as m from "@/paraglide/messages";

import { EmptyBox, Section } from "./section";

type ErrorBody = components["schemas"]["ErrorBody"];

export function PasskeysSection() {
  const passkeys = usePasskeys();
  const remove = useRemovePasskey();
  const formatDate = useDateTimeFormatter();
  const [adding, setAdding] = useState(false);
  const [renaming, setRenaming] = useState<Passkey | null>(null);
  const supported = isPasskeySupported();

  const handleRemove = (passkey: Passkey) => {
    remove.mutate(passkey.id, {
      onError: (error) =>
        toast.add({
          description: apiErrorMessage(error, m.common_try_again()),
          title: m.user_passkeys_remove_failed(),
          type: "error",
        }),
      onSuccess: () =>
        toast.add({ title: m.user_passkeys_removed(), type: "success" }),
    });
  };

  const renderBody = () => {
    if (passkeys.isPending) {
      return <Skeleton className="h-16 w-full" />;
    }
    if (passkeys.isError) {
      return (
        <p className="text-destructive text-sm">{m.user_passkeys_error()}</p>
      );
    }
    if (passkeys.data.length === 0) {
      return (
        <EmptyBox icon={<IconShieldLock size={20} />}>
          {m.user_passkeys_empty()}
        </EmptyBox>
      );
    }
    return (
      <ul className="space-y-3">
        {passkeys.data.map((passkey) => (
          <li
            className="flex flex-wrap items-start gap-x-3 gap-y-3 rounded-lg border p-3"
            key={passkey.id}
          >
            <span className="mt-0.5 shrink-0 text-muted-foreground">
              {passkey.backed_up ? (
                <IconDeviceMobile size={18} />
              ) : (
                <IconKey size={18} />
              )}
            </span>
            <div className="min-w-0 flex-1 basis-48 space-y-1">
              <div className="flex flex-wrap items-center gap-2">
                <span className="break-all font-medium text-sm">
                  {passkey.name}
                </span>
                {passkey.backed_up ? (
                  <Badge variant="outline">{m.user_passkeys_synced()}</Badge>
                ) : null}
              </div>
              <p className="text-muted-foreground text-xs">
                {m.user_passkeys_added_at({
                  date: formatDate(passkey.created_at),
                })}
                {" · "}
                {passkey.last_used_at
                  ? m.user_passkeys_last_used({
                      date: formatDate(passkey.last_used_at),
                    })
                  : m.user_passkeys_never_used()}
              </p>
            </div>
            <div className="flex w-full gap-2 sm:w-auto">
              <Button
                className="flex-1 sm:flex-none"
                onClick={() => setRenaming(passkey)}
                size="sm"
                type="button"
                variant="outline"
              >
                <IconPencil size={16} />
                {m.user_passkeys_rename()}
              </Button>
              <Button
                className="flex-1 sm:flex-none"
                disabled={remove.isPending && remove.variables === passkey.id}
                onClick={() => handleRemove(passkey)}
                size="sm"
                type="button"
                variant="outline"
              >
                <IconTrash size={16} />
                {m.common_delete()}
              </Button>
            </div>
          </li>
        ))}
      </ul>
    );
  };

  return (
    <Section
      action={
        <Button
          disabled={!supported}
          onClick={() => setAdding(true)}
          size="sm"
          type="button"
          variant="outline"
        >
          <IconPlus size={16} />
          {m.user_passkeys_add()}
        </Button>
      }
      description={m.user_passkeys_description()}
      title={m.user_passkeys_title()}
    >
      {supported ? null : (
        <p className="text-muted-foreground text-sm">
          {m.user_passkeys_unsupported()}
        </p>
      )}
      {renderBody()}
      <ResponsiveDialog onOpenChange={setAdding} open={adding}>
        <ResponsiveDialogContent>
          {adding ? <AddPasskeyForm onDone={() => setAdding(false)} /> : null}
        </ResponsiveDialogContent>
      </ResponsiveDialog>
      <ResponsiveDialog
        onOpenChange={(open) => (open ? null : setRenaming(null))}
        open={renaming !== null}
      >
        <ResponsiveDialogContent>
          {renaming ? (
            <RenamePasskeyForm
              key={renaming.id}
              onDone={() => setRenaming(null)}
              passkey={renaming}
            />
          ) : null}
        </ResponsiveDialogContent>
      </ResponsiveDialog>
    </Section>
  );
}

function AddPasskeyForm({ onDone }: { onDone: () => void }) {
  const hasPassword = useHasPassword();
  const add = useAddPasskey();
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  // Closing the browser's own prompt is not a failure worth red text.
  const error =
    add.error && !isPasskeyCancelled(add.error)
      ? (add.error as Partial<ErrorBody>)
      : null;
  const needsSignIn = error?.code === "reauthentication_required";

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    add.mutate(
      {
        currentPassword: hasPassword ? password : undefined,
        name: name.trim(),
      },
      {
        onSuccess: () => {
          toast.add({ title: m.user_passkeys_added(), type: "success" });
          onDone();
        },
      }
    );
  };

  return (
    <form className="contents" onSubmit={handleSubmit}>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {m.user_passkeys_add_title()}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {m.user_passkeys_add_description()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>
      <div className="space-y-4">
        <Field>
          <FieldLabel htmlFor="passkey-name">
            {m.user_passkeys_name()}
          </FieldLabel>
          <Input
            // oxlint-disable-next-line jsx-a11y/no-autofocus -- first input in the dialog
            autoFocus
            id="passkey-name"
            maxLength={64}
            onChange={(event) => setName(event.target.value)}
            placeholder={m.user_passkeys_name_placeholder()}
            value={name}
          />
        </Field>
        {hasPassword ? (
          <Field data-invalid={Boolean(error) && !needsSignIn}>
            <FieldLabel htmlFor="passkey-password">
              {m.user_password_current()}
            </FieldLabel>
            <Input
              autoComplete="current-password"
              id="passkey-password"
              onChange={(event) => setPassword(event.target.value)}
              type="password"
              value={password}
            />
          </Field>
        ) : null}
        {error && !needsSignIn ? (
          <FieldError>
            {apiErrorMessage(error, m.common_try_again())}
          </FieldError>
        ) : null}
        {needsSignIn ? (
          <SignInAgain message={m.user_reauth_required()} />
        ) : null}
      </div>
      <ResponsiveDialogFooter>
        <Button onClick={onDone} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={add.isPending || (hasPassword && password.length === 0)}
          type="submit"
        >
          {add.isPending ? m.user_passkeys_adding() : m.user_passkeys_add()}
        </Button>
      </ResponsiveDialogFooter>
    </form>
  );
}

function RenamePasskeyForm({
  passkey,
  onDone,
}: {
  passkey: Passkey;
  onDone: () => void;
}) {
  const rename = useRenamePasskey();
  const [name, setName] = useState(passkey.name);
  const trimmed = name.trim();

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    rename.mutate({ id: passkey.id, name: trimmed }, { onSuccess: onDone });
  };

  return (
    <form className="contents" onSubmit={handleSubmit}>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {m.user_passkeys_rename_title()}
        </ResponsiveDialogTitle>
      </ResponsiveDialogHeader>
      <Field data-invalid={Boolean(rename.error)}>
        <FieldLabel htmlFor="passkey-rename">
          {m.user_passkeys_name()}
        </FieldLabel>
        <Input
          // oxlint-disable-next-line jsx-a11y/no-autofocus -- only input in the dialog
          autoFocus
          id="passkey-rename"
          maxLength={64}
          onChange={(event) => setName(event.target.value)}
          value={name}
        />
        {rename.error ? (
          <FieldError>
            {apiErrorMessage(rename.error, m.common_try_again())}
          </FieldError>
        ) : null}
      </Field>
      <ResponsiveDialogFooter>
        <Button onClick={onDone} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={
            rename.isPending || trimmed === "" || trimmed === passkey.name
          }
          type="submit"
        >
          {rename.isPending ? m.common_saving() : m.common_save()}
        </Button>
      </ResponsiveDialogFooter>
    </form>
  );
}
