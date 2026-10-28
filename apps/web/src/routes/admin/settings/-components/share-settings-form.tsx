import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Textarea } from "@tessera/ui/components/textarea";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import {
  CODE_LENGTH_MAX,
  CODE_LENGTH_MIN,
  shareSettingsSchema,
} from "@/lib/admin/settings-schema";
import type { ShareSettingsValues } from "@/lib/admin/settings-schema";
import {
  asBoolean,
  asLines,
  asNumber,
  toLines,
} from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { SettingsNotice, SwitchField } from "./form-fields";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_CODE_LENGTH = 8;
const DEFAULT_ACCESS_LOG_RETENTION_DAYS = 30;
const DEFAULT_INLINE_THRESHOLD_BYTES = 64 * 1024;
const DEFAULT_MAX_TEXT_BYTES = 1024 * 1024;
const DEFAULT_TICKET_TTL_HOURS = 24;
const DEFAULT_TICKET_IDLE_HOURS = 1;
const DEFAULT_RETAIN_AFTER_DEATH_HOURS = 24 * 7;
const DEFAULT_UPLOAD_SESSION_TTL_HOURS = 24;

const KEYS = {
  accessLogEnabled: "share.access_log.enabled",
  accessLogRetentionDays: "share.access_log.retention_days",
  blockedExtensions: "share.blocked_extensions",
  codeLength: "share.code_length",
  encryption: "features.encryption",
  inlineThresholdBytes: "share.inline_threshold_bytes",
  maxTextBytes: "share.max_text_bytes",
  retainAfterDeathHours: "share.retain_after_death_hours",
  stripMetadata: "share.strip_metadata",
  ticketIdleHours: "share.ticket_idle_hours",
  ticketTtlHours: "share.ticket_ttl_hours",
  uploadSessionTtlHours: "share.upload_session_ttl_hours",
} satisfies SettingsKeyMap<ShareSettingsValues>;

const ENCODERS = {
  blockedExtensions: (value: string) => toLines(value),
};

export function ShareSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <ShareSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): ShareSettingsValues {
  return {
    accessLogEnabled: asBoolean(
      readSetting(settings, KEYS.accessLogEnabled),
      false
    ),
    accessLogRetentionDays: asNumber(
      readSetting(settings, KEYS.accessLogRetentionDays),
      DEFAULT_ACCESS_LOG_RETENTION_DAYS
    ),
    blockedExtensions: asLines(
      readSetting(settings, KEYS.blockedExtensions),
      ""
    ),
    codeLength: asNumber(
      readSetting(settings, KEYS.codeLength),
      DEFAULT_CODE_LENGTH
    ),
    encryption: asBoolean(readSetting(settings, KEYS.encryption), false),
    inlineThresholdBytes: asNumber(
      readSetting(settings, KEYS.inlineThresholdBytes),
      DEFAULT_INLINE_THRESHOLD_BYTES
    ),
    maxTextBytes: asNumber(
      readSetting(settings, KEYS.maxTextBytes),
      DEFAULT_MAX_TEXT_BYTES
    ),
    retainAfterDeathHours: asNumber(
      readSetting(settings, KEYS.retainAfterDeathHours),
      DEFAULT_RETAIN_AFTER_DEATH_HOURS
    ),
    stripMetadata: asBoolean(readSetting(settings, KEYS.stripMetadata), true),
    ticketIdleHours: asNumber(
      readSetting(settings, KEYS.ticketIdleHours),
      DEFAULT_TICKET_IDLE_HOURS
    ),
    ticketTtlHours: asNumber(
      readSetting(settings, KEYS.ticketTtlHours),
      DEFAULT_TICKET_TTL_HOURS
    ),
    uploadSessionTtlHours: asNumber(
      readSetting(settings, KEYS.uploadSessionTtlHours),
      DEFAULT_UPLOAD_SESSION_TTL_HOURS
    ),
  };
}

function ShareSettingsFields({
  defaultValues,
}: {
  defaultValues: ShareSettingsValues;
}) {
  const form = useSettingsForm<ShareSettingsValues>({
    defaultValues,
    encoders: ENCODERS,
    keys: KEYS,
    resolver: zodResolver(shareSettingsSchema),
  });

  const accessLogEnabled = useWatch({
    control: form.control,
    name: "accessLogEnabled",
  });

  return (
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      <SettingsCard
        description={m.settings_share_encryption_description()}
        title={m.settings_share_encryption_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="encryption"
            render={({ field }) => (
              <SwitchField
                description={m.settings_share_form_encryption_description()}
                field={field}
                id="share-encryption"
                label={m.settings_share_form_encryption_label()}
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_share_code_description()}
        title={m.settings_share_code_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="codeLength"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_share_form_code_length_description()}
                field={field}
                fieldState={fieldState}
                id="share-code-length"
                label={m.settings_share_form_code_length_label()}
                max={CODE_LENGTH_MAX}
                min={CODE_LENGTH_MIN}
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_share_text_description()}
        title={m.settings_share_text_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="maxTextBytes"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_share_form_max_text_bytes_description()}
                field={field}
                fieldState={fieldState}
                id="share-max-text-bytes"
                label={m.settings_share_form_max_text_bytes_label()}
                min={1}
                unit="bytes"
              />
            )}
          />

          <Controller
            control={form.control}
            name="inlineThresholdBytes"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_share_form_inline_threshold_description()}
                field={field}
                fieldState={fieldState}
                id="share-inline-threshold"
                label={m.settings_share_form_inline_threshold_label()}
                min={0}
                unit="bytes"
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_share_blocked_description()}
        title={m.settings_share_blocked_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="blockedExtensions"
            render={({ field, fieldState }) => (
              <Field data-invalid={fieldState.invalid}>
                <FieldLabel htmlFor="share-blocked-extensions">
                  {m.settings_share_form_blocked_extensions_label()}
                </FieldLabel>
                <Textarea
                  {...field}
                  aria-invalid={fieldState.invalid}
                  className="font-mono"
                  id="share-blocked-extensions"
                  placeholder={"exe\napk\nscr"}
                  rows={4}
                />
                <FieldDescription>
                  {m.settings_share_form_blocked_extensions_description()}
                </FieldDescription>
                {fieldState.invalid && (
                  <FieldError errors={[fieldState.error]} />
                )}
              </Field>
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_share_metadata_description()}
        title={m.settings_share_metadata_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="stripMetadata"
            render={({ field }) => (
              <SwitchField
                description={m.settings_share_form_strip_metadata_description()}
                field={field}
                id="share-strip-metadata"
                label={m.settings_share_form_strip_metadata_label()}
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_share_lifecycle_description()}
        title={m.settings_share_lifecycle_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="ticketTtlHours"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_share_form_ticket_ttl_description()}
                field={field}
                fieldState={fieldState}
                id="share-ticket-ttl"
                label={m.settings_share_form_ticket_ttl_label()}
                min={1}
                unit="hours"
              />
            )}
          />

          <Controller
            control={form.control}
            name="ticketIdleHours"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_share_form_ticket_idle_description()}
                field={field}
                fieldState={fieldState}
                id="share-ticket-idle"
                label={m.settings_share_form_ticket_idle_label()}
                min={1}
                unit="hours"
              />
            )}
          />

          <Controller
            control={form.control}
            name="uploadSessionTtlHours"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_share_form_upload_session_ttl_description()}
                field={field}
                fieldState={fieldState}
                id="share-upload-session-ttl"
                label={m.settings_share_form_upload_session_ttl_label()}
                min={1}
                unit="hours"
              />
            )}
          />

          <Controller
            control={form.control}
            name="retainAfterDeathHours"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_share_form_retain_after_death_description()}
                field={field}
                fieldState={fieldState}
                id="share-retain-after-death"
                label={m.settings_share_form_retain_after_death_label()}
                min={0}
                unit="hours"
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_share_access_log_description()}
        title={m.settings_share_access_log_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="accessLogEnabled"
            render={({ field }) => (
              <SwitchField
                description={m.settings_share_form_access_log_enabled_description()}
                field={field}
                id="share-access-log-enabled"
                label={m.settings_share_form_access_log_enabled_label()}
              />
            )}
          />

          {accessLogEnabled && (
            <SettingsNotice tone="info">
              {m.settings_share_form_access_log_notice()}
            </SettingsNotice>
          )}

          <Controller
            control={form.control}
            name="accessLogRetentionDays"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_share_form_access_log_retention_description()}
                field={field}
                fieldState={fieldState}
                id="share-access-log-retention"
                label={m.settings_share_form_access_log_retention_label()}
                min={0}
                unit="days"
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsFormFooter form={form} />
    </form>
  );
}
