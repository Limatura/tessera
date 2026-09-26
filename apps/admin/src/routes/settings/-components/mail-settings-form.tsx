import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup, FieldSeparator } from "@tessera/ui/components/field";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import {
  MAX_REGISTRATION_TTL_HOURS,
  MAX_RESET_TTL_MINUTES,
  mailSettingsSchema,
} from "@/lib/settings-schema";
import type { MailSettingsValues } from "@/lib/settings-schema";
import { asNumber, asString, isSecretSet } from "@/lib/settings-values";
import * as m from "@/paraglide/messages";

import { SealedField, TextField } from "./form-fields";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_PORT = 587;
const DEFAULT_RESET_TTL_MINUTES = 30;
const DEFAULT_REGISTRATION_TTL_HOURS = 24;
const MAX_PORT = 65_535;

const KEYS = {
  from: "mail.from",
  host: "mail.host",
  password: "mail.password",
  port: "mail.port",
  publicUrl: "mail.public_url",
  registrationTtlHours: "mail.registration_ttl_hours",
  resetTtlMinutes: "mail.reset_ttl_minutes",
  username: "mail.username",
} satisfies SettingsKeyMap<MailSettingsValues>;

const ENCODERS = {
  // SEALED：留空代表不改動，所以不要送這個 key。
  password: (value: string) => (value === "" ? undefined : value),
  username: (value: string) => (value === "" ? undefined : value),
};

/**
 * 寄信設定。空的 `mail.host` = 完全關閉，這也是預設值。
 *
 * 關著的話重設密碼、註冊確認信都寄不出去 —— 單人實例可以在機器上直接跑
 * `reset-password` 回到忘記的帳號，但有其他使用者的實例就需要一個 relay。
 */
export function MailSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <MailSettingsFields
          defaultValues={initialValues(settings)}
          passwordIsSet={isSecretSet(settings, KEYS.password)}
          usernameIsSet={isSecretSet(settings, KEYS.username)}
        />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): MailSettingsValues {
  return {
    from: asString(readSetting(settings, KEYS.from), ""),
    host: asString(readSetting(settings, KEYS.host), ""),
    // SEALED：讀不回值，永遠從空的開始。
    password: "",
    port: asNumber(readSetting(settings, KEYS.port), DEFAULT_PORT),
    publicUrl: asString(readSetting(settings, KEYS.publicUrl), ""),
    registrationTtlHours: asNumber(
      readSetting(settings, KEYS.registrationTtlHours),
      DEFAULT_REGISTRATION_TTL_HOURS
    ),
    resetTtlMinutes: asNumber(
      readSetting(settings, KEYS.resetTtlMinutes),
      DEFAULT_RESET_TTL_MINUTES
    ),
    username: "",
  };
}

function MailSettingsFields({
  defaultValues,
  usernameIsSet,
  passwordIsSet,
}: {
  defaultValues: MailSettingsValues;
  usernameIsSet: boolean;
  passwordIsSet: boolean;
}) {
  const form = useSettingsForm<MailSettingsValues>({
    defaultValues,
    encoders: ENCODERS,
    keys: KEYS,
    resolver: zodResolver(mailSettingsSchema),
  });

  const host = useWatch({ control: form.control, name: "host" });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        {host.trim() === "" && (
          <p className="rounded-md border border-border bg-muted/40 p-3 text-muted-foreground text-sm">
            {m.settings_mail_form_host_empty_warning()}
          </p>
        )}

        <Controller
          control={form.control}
          name="host"
          render={({ field, fieldState }) => (
            <TextField
              description={m.settings_mail_form_host_description()}
              field={field}
              fieldState={fieldState}
              id="mail-host"
              label={m.settings_mail_form_host_label()}
              placeholder="smtp.resend.com"
            />
          )}
        />

        <Controller
          control={form.control}
          name="port"
          render={({ field, fieldState }) => (
            <NumberField
              description={m.settings_mail_form_port_description()}
              field={field}
              fieldState={fieldState}
              id="mail-port"
              label={m.settings_mail_form_port_label()}
              max={MAX_PORT}
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="username"
          render={({ field, fieldState }) => (
            <SealedField
              description={m.settings_mail_form_username_description()}
              field={field}
              fieldState={fieldState}
              id="mail-username"
              isSet={usernameIsSet}
              label={m.settings_mail_form_username_label()}
            />
          )}
        />

        <Controller
          control={form.control}
          name="password"
          render={({ field, fieldState }) => (
            <SealedField
              description={m.settings_mail_form_password_description()}
              field={field}
              fieldState={fieldState}
              id="mail-password"
              isSet={passwordIsSet}
              label={m.settings_mail_form_password_label()}
            />
          )}
        />

        <FieldSeparator />

        <Controller
          control={form.control}
          name="from"
          render={({ field, fieldState }) => (
            <TextField
              description={m.settings_mail_form_from_description()}
              field={field}
              fieldState={fieldState}
              id="mail-from"
              label={m.settings_mail_form_from_label()}
              placeholder="tessera@example.com"
            />
          )}
        />

        <Controller
          control={form.control}
          name="publicUrl"
          render={({ field, fieldState }) => (
            <TextField
              description={m.settings_mail_form_public_url_description()}
              field={field}
              fieldState={fieldState}
              id="mail-public-url"
              label={m.settings_mail_form_public_url_label()}
              placeholder="https://tessera.example.com"
            />
          )}
        />

        <FieldSeparator />

        <Controller
          control={form.control}
          name="resetTtlMinutes"
          render={({ field, fieldState }) => (
            <NumberField
              description={m.settings_mail_form_reset_ttl_description()}
              field={field}
              fieldState={fieldState}
              id="mail-reset-ttl"
              label={m.settings_mail_form_reset_ttl_label()}
              max={MAX_RESET_TTL_MINUTES}
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="registrationTtlHours"
          render={({ field, fieldState }) => (
            <NumberField
              description={m.settings_mail_form_registration_ttl_description()}
              field={field}
              fieldState={fieldState}
              id="mail-registration-ttl"
              label={m.settings_mail_form_registration_ttl_label()}
              max={MAX_REGISTRATION_TTL_HOURS}
              min={1}
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
