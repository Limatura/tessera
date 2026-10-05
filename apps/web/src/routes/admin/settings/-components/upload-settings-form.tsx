import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup, FieldSeparator } from "@tessera/ui/components/field";
import { Controller } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import { uploadSettingsSchema } from "@/lib/admin/settings-schema";
import type { UploadSettingsValues } from "@/lib/admin/settings-schema";
import { asBoolean, asNumber } from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { SwitchField } from "./form-fields";
import { GuestAnonymousWarning } from "./guest-anonymous-warning";
import { NumberField } from "./number-field";
import { ReadOnlySetting } from "./read-only-setting";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_THROTTLE_WINDOW_SECS = 300;
const DEFAULT_THROTTLE_MAX_ATTEMPTS = 20;
const DEFAULT_DEFAULT_EXPIRY_HOURS = 24 * 7;
const DEFAULT_MAX_EXPIRY_HOURS = 24 * 30;

/** 唯讀顯示用，不進表單 —— 它在 FIXED_AT_BOOT，送出去會被整批退回。 */
const MAX_BODY_BYTES_KEY = "server.max_body_bytes";

const KEYS = {
  allowAnonymousUpload: "features.anonymous_upload",
  defaultExpiryHours: "share.default_expiry_hours",
  maxExpiryHours: "share.max_expiry_hours",
  maxTotalBytes: "storage.max_total_bytes",
  throttleMaxAttempts: "auth.throttle.source.max_attempts",
  throttleWindowSecs: "auth.throttle.source.window_secs",
} satisfies SettingsKeyMap<UploadSettingsValues>;

export function UploadSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <UploadSettingsFields
          defaultValues={initialValues(settings)}
          maxBodyBytes={asNumber(readSetting(settings, MAX_BODY_BYTES_KEY), 0)}
        />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): UploadSettingsValues {
  return {
    allowAnonymousUpload: asBoolean(
      readSetting(settings, KEYS.allowAnonymousUpload),
      true
    ),
    defaultExpiryHours: asNumber(
      readSetting(settings, KEYS.defaultExpiryHours),
      DEFAULT_DEFAULT_EXPIRY_HOURS
    ),
    maxExpiryHours: asNumber(
      readSetting(settings, KEYS.maxExpiryHours),
      DEFAULT_MAX_EXPIRY_HOURS
    ),
    maxTotalBytes: asNumber(readSetting(settings, KEYS.maxTotalBytes), 0),
    throttleMaxAttempts: asNumber(
      readSetting(settings, KEYS.throttleMaxAttempts),
      DEFAULT_THROTTLE_MAX_ATTEMPTS
    ),
    throttleWindowSecs: asNumber(
      readSetting(settings, KEYS.throttleWindowSecs),
      DEFAULT_THROTTLE_WINDOW_SECS
    ),
  };
}

function UploadSettingsFields({
  defaultValues,
  maxBodyBytes,
}: {
  defaultValues: UploadSettingsValues;
  maxBodyBytes: number;
}) {
  const form = useSettingsForm<UploadSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(uploadSettingsSchema),
  });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <GuestAnonymousWarning fixAt="account" />

        <Controller
          control={form.control}
          name="throttleWindowSecs"
          render={({ field, fieldState }) => (
            <NumberField
              description={m.settings_upload_form_throttle_window_description()}
              field={field}
              fieldState={fieldState}
              id="upload-throttle-window"
              label={m.settings_upload_form_throttle_window_label()}
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="throttleMaxAttempts"
          render={({ field, fieldState }) => (
            <NumberField
              description={m.settings_upload_form_throttle_attempts_description()}
              field={field}
              fieldState={fieldState}
              id="upload-throttle-attempts"
              label={m.settings_upload_form_throttle_attempts_label()}
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="allowAnonymousUpload"
          render={({ field }) => (
            <SwitchField
              description={m.settings_upload_form_allow_anonymous_description()}
              field={field}
              id="upload-allow-anonymous"
              label={m.settings_upload_form_allow_anonymous_label()}
            />
          )}
        />

        <ReadOnlySetting
          description={
            <>
              {m.settings_upload_form_max_body_description_before()}{" "}
              <code>TESSERA_SERVER__MAX_BODY_BYTES</code>{" "}
              {m.settings_upload_form_max_body_description_after()}
            </>
          }
          label={m.settings_upload_form_max_body_label()}
          reason={m.settings_upload_form_max_body_reason()}
          value={maxBodyBytes.toLocaleString()}
        />

        <FieldSeparator />

        <Controller
          control={form.control}
          name="maxTotalBytes"
          render={({ field, fieldState }) => (
            <NumberField
              description={m.settings_upload_form_max_total_bytes_description()}
              field={field}
              fieldState={fieldState}
              id="upload-max-total-bytes"
              label={m.settings_upload_form_max_total_bytes_label()}
              min={0}
            />
          )}
        />

        <Controller
          control={form.control}
          name="defaultExpiryHours"
          render={({ field, fieldState }) => (
            <NumberField
              description={m.settings_upload_form_default_expiry_description()}
              field={field}
              fieldState={fieldState}
              id="upload-default-expiry"
              label={m.settings_upload_form_default_expiry_label()}
              min={0}
            />
          )}
        />

        <Controller
          control={form.control}
          name="maxExpiryHours"
          render={({ field, fieldState }) => (
            <NumberField
              description={m.settings_upload_form_max_expiry_description()}
              field={field}
              fieldState={fieldState}
              id="upload-max-expiry"
              label={m.settings_upload_form_max_expiry_label()}
              min={0}
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
