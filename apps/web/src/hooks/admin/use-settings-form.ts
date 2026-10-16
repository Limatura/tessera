import { useSession } from "@tessera/auth/hooks";
import { useState } from "react";
import type { DefaultValues, FieldValues, UseFormProps } from "react-hook-form";
import { useForm } from "react-hook-form";

import {
  usePreviewChanges,
  useUpdateSettings,
} from "@/hooks/admin/use-settings";
import type { Consequence } from "@/lib/admin/consequences";
import { PERMISSIONS, hasPermission } from "@/lib/admin/permissions";
import { apiErrorCode, apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

/** Form field name → server dotted key. Omitted fields are never sent. */
export type SettingsKeyMap<TValues extends FieldValues> = Partial<
  Record<keyof TValues & string, string>
>;

/**
 * Transforms applied before sending. `undefined` means "do not send this key
 * this time": an empty SEALED field takes this path, since an unfilled secret
 * must not be overwritten with an empty string.
 */
export type SettingsEncoders<TValues extends FieldValues> = Partial<{
  [K in keyof TValues]: (value: TValues[K]) => unknown;
}>;

/**
 * Picks only the fields that actually changed. `PUT /admin/settings` validates
 * the changes as one batch and one invalid key rejects all of them, so untouched
 * fields are not sent even when their values are valid.
 */
export function collectChanges<TValues extends FieldValues>(
  keys: SettingsKeyMap<TValues>,
  dirtyFields: Record<string, unknown>,
  values: TValues,
  encoders?: SettingsEncoders<TValues>
): Record<string, unknown> {
  const changes: Record<string, unknown> = {};

  for (const name of Object.keys(keys) as (keyof TValues & string)[]) {
    const key = keys[name];
    if (!(key && dirtyFields[name])) {
      continue;
    }

    const encode = encoders?.[name];
    const encoded = encode ? encode(values[name]) : values[name];
    if (encoded === undefined) {
      continue;
    }

    changes[key] = encoded;
  }

  return changes;
}

/** The server's explanation when `PUT /admin/settings` fails. */
export function settingsErrorMessage(error: unknown): string | null {
  if (!error) {
    return null;
  }
  return apiErrorMessage(error, m.settings_form_save_failed());
}

/**
 * The half every settings form shares: react-hook-form, the write permission,
 * and a submit that sends only dirty fields. Initial values come from the
 * caller, read from `GET /admin/settings`, and are read only on mount so a
 * background refetch does not wipe what the user is typing.
 */
export function useSettingsForm<TValues extends FieldValues>({
  defaultValues,
  resolver,
  keys,
  encoders,
}: {
  defaultValues: TValues;
  resolver: UseFormProps<TValues>["resolver"];
  keys: SettingsKeyMap<TValues>;
  encoders?: SettingsEncoders<TValues>;
}) {
  const { data: account } = useSession();
  const update = useUpdateSettings();
  const preview = usePreviewChanges();
  // A save the server refused until confirmed: what it would delete, and what
  // to send again once the admin agrees.
  const [held, setHeld] = useState<{
    changes: Record<string, unknown>;
    values: TValues;
    consequences: Consequence[];
  } | null>(null);
  const form = useForm<TValues>({
    defaultValues: defaultValues as DefaultValues<TValues>,
    resolver,
  });

  // Read during render so react-hook-form subscribes to both. Touching
  // `dirtyFields` only in the submit callback means it is never computed.
  const { dirtyFields, isDirty } = form.formState;

  const submit = form.handleSubmit((values) => {
    const changes = collectChanges(
      keys,
      dirtyFields as Record<string, unknown>,
      values,
      encoders
    );

    // Empty if the only change was a SEALED field that was cleared again. reset
    // returns the form to a clean saved state instead of leaving a dead button.
    if (Object.keys(changes).length === 0) {
      form.reset(values);
      return;
    }

    update.mutate(
      { changes },
      {
        onError: (error) => {
          // A shorter retention period deletes what is already stored. Ask the
          // server for the numbers and hold the save until the admin agrees.
          if (apiErrorCode(error) !== "confirmation_required") {
            return;
          }
          preview.mutate(changes, {
            onSuccess: (consequences) =>
              setHeld({ changes, consequences, values }),
          });
        },
        onSuccess: () => form.reset(values),
      }
    );
  });

  const confirmation = held && {
    consequences: held.consequences,
    isPending: update.isPending || preview.isPending,
    /** Sends the held changes again, this time with `confirm`. */
    confirm: () =>
      update.mutate(
        { changes: held.changes, confirm: true },
        {
          onSuccess: () => {
            form.reset(held.values);
            setHeld(null);
          },
        }
      ),
    /** Keeps the edits in the form, unsaved. */
    cancel: () => {
      setHeld(null);
      update.reset();
    },
  };

  // The refusal is not a failure to show while it is being asked about; a
  // failed preview is, since then there are no numbers to show.
  const refusedForConfirmation =
    apiErrorCode(update.error) === "confirmation_required" &&
    (held !== null || preview.isPending);
  const errorMessage = refusedForConfirmation
    ? null
    : settingsErrorMessage(update.error ?? preview.error);

  return {
    canWrite: hasPermission(account, PERMISSIONS.settingsWrite),
    control: form.control,
    /** Back to the last saved values, clearing the last submit error too. */
    /** Set while a save waits for the admin to accept what it deletes. */
    confirmation,
    discard: () => {
      form.reset();
      update.reset();
      preview.reset();
      setHeld(null);
    },
    errorMessage,
    isDirty,
    isPending: update.isPending || preview.isPending,
    isSaved: update.isSuccess && !isDirty,
    // For fields derived from another field (currently only the OIDC discovery
    // URL). Callers must pass `shouldDirty`: submit sends only dirty fields, so
    // an unmarked value would show on screen but never be saved, and that is
    // worse than not filling it in.
    setValue: form.setValue,
    submit,
  };
}
