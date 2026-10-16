import { zodResolver } from "@hookform/resolvers/zod";
import { Link } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { FieldGroup } from "@tessera/ui/components/field";
import { formatBytes } from "@tessera/ui/lib/format";
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
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_DEFAULT_EXPIRY_HOURS = 24 * 7;
const DEFAULT_MAX_EXPIRY_HOURS = 24 * 30;

/** Shown read-only, never submitted: it is FIXED_AT_BOOT and would get the batch rejected. */
const MAX_BODY_BYTES_KEY = "server.max_body_bytes";

const KEYS = {
  allowAnonymousUpload: "features.anonymous_upload",
  defaultExpiryHours: "share.default_expiry_hours",
  maxExpiryHours: "share.max_expiry_hours",
  maxTotalBytes: "storage.max_total_bytes",
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
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      <GuestAnonymousWarning fixAt="account" />

      <SettingsCard
        description={m.settings_upload_who_description()}
        title={m.settings_upload_who_title()}
      >
        <FieldGroup>
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
          <p className="text-muted-foreground text-sm">
            {m.settings_upload_rate_limit_moved()}{" "}
            <Link
              className="text-primary underline underline-offset-4 transition-colors hover:text-primary"
              to="/admin/settings/throttle"
            >
              {m.nav_settings_throttle()}
            </Link>
          </p>
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_upload_expiry_description()}
        title={m.settings_upload_expiry_title()}
      >
        <FieldGroup>
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
                unit="hours"
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
                unit="hours"
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_upload_size_description()}
        title={m.settings_upload_size_title()}
      >
        <FieldGroup>
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
                unit="bytes"
              />
            )}
          />

          <ReadOnlySetting
            description={
              <>
                {m.settings_upload_form_max_body_description()}{" "}
                <code>TESSERA_SERVER__MAX_BODY_BYTES</code>
              </>
            }
            label={m.settings_upload_form_max_body_label()}
            reason={m.settings_upload_form_max_body_reason()}
            value={formatBytes(maxBodyBytes)}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsFormFooter form={form} />
    </form>
  );
}
