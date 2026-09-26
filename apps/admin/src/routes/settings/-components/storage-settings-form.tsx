import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup, FieldSeparator } from "@tessera/ui/components/field";
import { Controller } from "react-hook-form";

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import {
  MIN_PART_SIZE_BYTES,
  storageSettingsSchema,
} from "@/lib/settings-schema";
import type { StorageSettingsValues } from "@/lib/settings-schema";
import { asNumber } from "@/lib/settings-values";
import * as m from "@/paraglide/messages";

import { NumberField } from "./number-field";
import { ReadOnlySetting } from "./read-only-setting";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_PART_SIZE_BYTES = 8 * 1024 * 1024;

const KEYS = {
  partSizeBytes: "server.part_size_bytes",
} satisfies SettingsKeyMap<StorageSettingsValues>;

export function StorageSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <StorageSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): StorageSettingsValues {
  return {
    partSizeBytes: asNumber(
      readSetting(settings, KEYS.partSizeBytes),
      DEFAULT_PART_SIZE_BYTES
    ),
  };
}

function StorageSettingsFields({
  defaultValues,
}: {
  defaultValues: StorageSettingsValues;
}) {
  const form = useSettingsForm<StorageSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(storageSettingsSchema),
  });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <ReadOnlySetting
          description={
            <>
              {m.settings_storage_form_url_description_1()}
              <code>s3://key:secret@…</code>
              {m.settings_storage_form_url_description_2()}{" "}
              <code>TESSERA_STORAGE__URL</code>{" "}
              {m.settings_storage_form_url_description_3()}
            </>
          }
          label={m.settings_storage_form_url_label()}
          reason={m.settings_storage_form_url_reason()}
          value={null}
        />

        <FieldSeparator />

        <Controller
          control={form.control}
          name="partSizeBytes"
          render={({ field, fieldState }) => (
            <NumberField
              description={m.settings_storage_form_part_size_description()}
              field={field}
              fieldState={fieldState}
              id="storage-part-size"
              label={m.settings_storage_form_part_size_label()}
              min={MIN_PART_SIZE_BYTES}
            />
          )}
        />

        <SettingsFormFooter
          canWrite={form.canWrite}
          errorMessage={form.errorMessage}
          isDirty={form.isDirty}
          isPending={form.isPending}
          isSaved={form.isSaved}
        />
      </FieldGroup>
    </form>
  );
}
