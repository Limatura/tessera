import { zodResolver } from "@hookform/resolvers/zod";
import { IconAlertTriangle } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { FieldGroup } from "@tessera/ui/components/field";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import {
  MIN_PART_SIZE_BYTES,
  storageSettingsSchema,
} from "@/lib/admin/settings-schema";
import type { StorageSettingsValues } from "@/lib/admin/settings-schema";
import { asBoolean, asNumber } from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { SettingsNotice, SwitchField } from "./form-fields";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_PART_SIZE_BYTES = 8 * 1024 * 1024;
const DEFAULT_RECONCILE_INTERVAL_HOURS = 24;
const DEFAULT_RECONCILE_GRACE_HOURS = 24;

const KEYS = {
  partSizeBytes: "server.part_size_bytes",
  reconcileGraceHours: "storage.reconcile_grace_hours",
  reconcileIntervalHours: "storage.reconcile_interval_hours",
  splitAcrossBackends: "storage.split_across_backends",
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
    reconcileGraceHours: asNumber(
      readSetting(settings, KEYS.reconcileGraceHours),
      DEFAULT_RECONCILE_GRACE_HOURS
    ),
    reconcileIntervalHours: asNumber(
      readSetting(settings, KEYS.reconcileIntervalHours),
      DEFAULT_RECONCILE_INTERVAL_HOURS
    ),
    splitAcrossBackends: asBoolean(
      readSetting(settings, KEYS.splitAcrossBackends),
      false
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

  const split = useWatch({
    control: form.control,
    name: "splitAcrossBackends",
  });
  const reconcileInterval = useWatch({
    control: form.control,
    name: "reconcileIntervalHours",
  });

  return (
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      <SettingsCard
        description={m.settings_storage_upload_description()}
        title={m.settings_storage_upload_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="splitAcrossBackends"
            render={({ field }) => (
              <SwitchField
                description={m.settings_storage_form_split_description()}
                field={field}
                id="storage-split"
                label={m.settings_storage_form_split_label()}
              />
            )}
          />

          {/*
            Split files cannot be served directly: a presigned URL points at one
            object, so the bytes pass through this server or are joined in the
            browser. On a free tier that costs upload and download bandwidth.
          */}
          {split && !defaultValues.splitAcrossBackends && (
            <SettingsNotice
              icon={<IconAlertTriangle />}
              title={m.settings_storage_form_split_warning_title()}
              tone="warning"
            >
              {m.settings_storage_form_split_warning_body()}
            </SettingsNotice>
          )}

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
                unit="bytes"
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_storage_reconcile_description()}
        title={m.settings_storage_reconcile_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="reconcileIntervalHours"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_storage_form_reconcile_interval_description()}
                field={field}
                fieldState={fieldState}
                id="storage-reconcile-interval"
                label={m.settings_storage_form_reconcile_interval_label()}
                min={0}
                unit="hours"
              />
            )}
          />

          {reconcileInterval === 0 && (
            <SettingsNotice tone="info">
              {m.settings_storage_form_reconcile_off_notice()}
            </SettingsNotice>
          )}

          <Controller
            control={form.control}
            name="reconcileGraceHours"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_storage_form_reconcile_grace_description()}
                field={field}
                fieldState={fieldState}
                id="storage-reconcile-grace"
                label={m.settings_storage_form_reconcile_grace_label()}
                min={0}
                unit="hours"
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsFormFooter form={form} />
    </form>
  );
}
