import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup } from "@tessera/ui/components/field";
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
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

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
    <SettingsCard
      description={m.settings_storage_description()}
      title={m.settings_storage_title()}
    >
      <form onSubmit={form.submit}>
        <FieldGroup>
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
    </SettingsCard>
  );
}
