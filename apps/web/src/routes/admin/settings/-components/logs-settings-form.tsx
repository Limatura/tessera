import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup } from "@tessera/ui/components/field";
import { Controller } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import { LOG_LEVELS } from "@/lib/admin/log-entries";
import {
  AUDIT_RETENTION_MAX_DAYS,
  LOG_RETENTION_MAX_DAYS,
  logsSettingsSchema,
} from "@/lib/admin/settings-schema";
import type { LogsSettingsValues } from "@/lib/admin/settings-schema";
import { asBoolean, asEnum, asNumber } from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { SelectField, SwitchField } from "./form-fields";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_RETENTION_DAYS = 3;
const DEFAULT_AUDIT_RETENTION_DAYS = 180;

const KEYS = {
  auditRecordShareEvents: "audit.record_share_events",
  auditRetentionDays: "audit.retention_days",
  minLevel: "logs.min_level",
  retentionDays: "logs.retention_days",
} satisfies SettingsKeyMap<LogsSettingsValues>;

/** A function, not a const: the labels must be read in the language at render time. */
function levelOptions() {
  return [
    { label: m.logs_level_trace(), value: "trace" },
    { label: m.logs_level_debug(), value: "debug" },
    { label: m.logs_level_info(), value: "info" },
    { label: m.logs_level_warn(), value: "warn" },
    { label: m.logs_level_error(), value: "error" },
  ] as const;
}

export function LogsSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <LogsSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): LogsSettingsValues {
  return {
    auditRecordShareEvents: asBoolean(
      readSetting(settings, KEYS.auditRecordShareEvents),
      true
    ),
    auditRetentionDays: asNumber(
      readSetting(settings, KEYS.auditRetentionDays),
      DEFAULT_AUDIT_RETENTION_DAYS
    ),
    minLevel: asEnum(readSetting(settings, KEYS.minLevel), LOG_LEVELS, "trace"),
    retentionDays: asNumber(
      readSetting(settings, KEYS.retentionDays),
      DEFAULT_RETENTION_DAYS
    ),
  };
}

function LogsSettingsFields({
  defaultValues,
}: {
  defaultValues: LogsSettingsValues;
}) {
  const form = useSettingsForm<LogsSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(logsSettingsSchema),
  });

  return (
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      <SettingsCard
        description={m.settings_logs_card_description()}
        title={m.settings_logs_card_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="retentionDays"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_logs_form_retention_description()}
                field={field}
                fieldState={fieldState}
                id="logs-retention-days"
                label={m.settings_logs_form_retention_label()}
                max={LOG_RETENTION_MAX_DAYS}
                min={1}
                unit="days"
              />
            )}
          />
          <Controller
            control={form.control}
            name="minLevel"
            render={({ field, fieldState }) => (
              <SelectField
                description={m.settings_logs_form_min_level_description()}
                field={field}
                fieldState={fieldState}
                id="logs-min-level"
                label={m.settings_logs_form_min_level_label()}
                options={levelOptions()}
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_audit_card_description()}
        title={m.settings_audit_card_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="auditRetentionDays"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_audit_retention_description()}
                field={field}
                fieldState={fieldState}
                id="audit-retention-days"
                label={m.settings_audit_retention_label()}
                max={AUDIT_RETENTION_MAX_DAYS}
                min={1}
                unit="days"
              />
            )}
          />
          <Controller
            control={form.control}
            name="auditRecordShareEvents"
            render={({ field }) => (
              <SwitchField
                description={m.settings_audit_share_events_description()}
                field={field}
                id="audit-record-share-events"
                label={m.settings_audit_share_events_label()}
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsFormFooter form={form} />
    </form>
  );
}
