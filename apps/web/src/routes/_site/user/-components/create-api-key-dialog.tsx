import { IconCheck, IconCopy } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { useSession } from "@tessera/auth/hooks";
import { Button } from "@tessera/ui/components/button";
import { Checkbox } from "@tessera/ui/components/checkbox";
import { Field, FieldError, FieldLabel } from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import {
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { useState } from "react";

import { useCreateToken } from "@/hooks/use-tokens";
import { apiErrorMessage } from "@/lib/api-error";
import { copyText } from "@/lib/copy-text";
import * as m from "@/paraglide/messages";

type CreatedToken = components["schemas"]["CreatedTokenResponse"];

const COPIED_FEEDBACK_MS = 2000;
const NEVER = "never";
const EXPIRY_CHOICES = ["30", "90", "365", NEVER] as const;
const DEFAULT_EXPIRY = "90";

/**
 * What a key can be asked for here. A key can never exceed the account that
 * mints it, so only the ones the account holds are offered. `tokens:manage`
 * and the admin permissions are left out on purpose: a key for an upload
 * script has no business with them.
 */
const GRANTABLE = ["shares:create", "shares:read", "shares:delete"] as const;
type Grantable = (typeof GRANTABLE)[number];

function permissionLabel(permission: Grantable): string {
  switch (permission) {
    case "shares:create": {
      return m.user_api_keys_perm_shares_create();
    }
    case "shares:read": {
      return m.user_api_keys_perm_shares_read();
    }
    case "shares:delete": {
      return m.user_api_keys_perm_shares_delete();
    }
    default: {
      return permission;
    }
  }
}

function expiryLabel(choice: string): string {
  switch (choice) {
    case "30": {
      return m.user_api_keys_expiry_30();
    }
    case "90": {
      return m.user_api_keys_expiry_90();
    }
    case "365": {
      return m.user_api_keys_expiry_365();
    }
    default: {
      return m.common_never_expires();
    }
  }
}

/**
 * Two steps in one dialog: the form, then the secret. The secret is kept in
 * the mutation result only; closing the dialog drops it for good.
 */
export function CreateApiKeyDialogBody({ onDone }: { onDone: () => void }) {
  const create = useCreateToken();
  const [created, setCreated] = useState<CreatedToken | null>(null);

  if (created) {
    return <SecretStep onDone={onDone} secret={created.secret} />;
  }
  return (
    <CreateForm
      error={create.error}
      isPending={create.isPending}
      onCancel={onDone}
      onSubmit={(body) =>
        create.mutate(body, { onSuccess: (token) => setCreated(token) })
      }
    />
  );
}

function CreateForm({
  error,
  isPending,
  onCancel,
  onSubmit,
}: {
  error: unknown;
  isPending: boolean;
  onCancel: () => void;
  onSubmit: (body: {
    name: string;
    permissions: string[];
    expires_in_days?: number;
  }) => void;
}) {
  const { data: session } = useSession();
  const held = new Set(session?.permissions);
  const offered = GRANTABLE.filter((permission) => held.has(permission));
  const [name, setName] = useState("");
  const [permissions, setPermissions] = useState<Set<string>>(
    new Set(["shares:create"])
  );
  const [expiry, setExpiry] = useState<string>(DEFAULT_EXPIRY);
  const trimmed = name.trim();

  const handleToggle = (permission: string, checked: boolean) => {
    setPermissions((current) => {
      const next = new Set(current);
      if (checked) {
        next.add(permission);
      } else {
        next.delete(permission);
      }
      return next;
    });
  };

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    onSubmit({
      expires_in_days: expiry === NEVER ? undefined : Number(expiry),
      name: trimmed,
      permissions: [...permissions],
    });
  };

  return (
    <form className="contents" onSubmit={handleSubmit}>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {m.user_api_keys_create_title()}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {m.user_api_keys_create_description()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>
      <div className="space-y-4">
        <Field>
          <FieldLabel htmlFor="api-key-name">
            {m.user_api_keys_name()}
          </FieldLabel>
          <Input
            // oxlint-disable-next-line jsx-a11y/no-autofocus -- first input in the dialog
            autoFocus
            id="api-key-name"
            maxLength={64}
            onChange={(event) => setName(event.target.value)}
            placeholder={m.user_api_keys_name_placeholder()}
            value={name}
          />
        </Field>
        <fieldset className="space-y-2">
          <legend className="font-medium text-sm">
            {m.user_api_keys_permissions()}
          </legend>
          {offered.map((permission) => (
            // oxlint-disable-next-line jsx-a11y/label-has-associated-control -- Checkbox is a base-ui button, so wrapping it in the label associates them
            <label
              className="flex cursor-pointer items-center gap-3 text-sm"
              key={permission}
            >
              <Checkbox
                checked={permissions.has(permission)}
                onCheckedChange={(checked) => handleToggle(permission, checked)}
              />
              {permissionLabel(permission)}
            </label>
          ))}
        </fieldset>
        <Field>
          <FieldLabel htmlFor="api-key-expiry">
            {m.user_api_keys_expiry()}
          </FieldLabel>
          <Select
            onValueChange={(value) => setExpiry(value as string)}
            value={expiry}
          >
            <SelectTrigger id="api-key-expiry">
              <SelectValue>{(value: string) => expiryLabel(value)}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {EXPIRY_CHOICES.map((choice) => (
                  <SelectItem key={choice} value={choice}>
                    {expiryLabel(choice)}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        {error ? (
          <FieldError>
            {apiErrorMessage(error, m.common_try_again())}
          </FieldError>
        ) : null}
      </div>
      <ResponsiveDialogFooter>
        <Button onClick={onCancel} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={isPending || trimmed === "" || permissions.size === 0}
          type="submit"
        >
          {isPending ? m.user_api_keys_creating() : m.user_api_keys_create()}
        </Button>
      </ResponsiveDialogFooter>
    </form>
  );
}

function SecretStep({
  secret,
  onDone,
}: {
  secret: string;
  onDone: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    if (!(await copyText(secret))) {
      return;
    }
    setCopied(true);
    setTimeout(() => setCopied(false), COPIED_FEEDBACK_MS);
  };

  return (
    <>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {m.user_api_keys_secret_title()}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {m.user_api_keys_secret_warning()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>
      <code className="block break-all rounded-lg border bg-muted/40 p-3 font-mono text-sm">
        {secret}
      </code>
      <ResponsiveDialogFooter>
        <Button onClick={handleCopy} type="button" variant="outline">
          {copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
          {copied ? m.user_api_keys_copied() : m.user_api_keys_copy()}
        </Button>
        <Button onClick={onDone} type="button">
          {m.user_api_keys_done()}
        </Button>
      </ResponsiveDialogFooter>
    </>
  );
}
