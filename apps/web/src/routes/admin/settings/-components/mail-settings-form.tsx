import { zodResolver } from "@hookform/resolvers/zod";
import { IconAlertTriangle, IconInfoCircle } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { FieldGroup } from "@tessera/ui/components/field";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import {
  MAIL_TLS_MODES,
  MAX_REGISTRATION_TTL_HOURS,
  MAX_RESET_TTL_MINUTES,
  mailSettingsSchema,
} from "@/lib/admin/settings-schema";
import type { MailSettingsValues } from "@/lib/admin/settings-schema";
import {
  asEnum,
  asNumber,
  asString,
  isSecretSet,
} from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import {
  SealedField,
  SelectField,
  SettingsNotice,
  TextField,
} from "./form-fields";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

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
  tls: "mail.tls",
  username: "mail.username",
} satisfies SettingsKeyMap<MailSettingsValues>;

/**
 * A function, not a module-level const: the `m.xxx()` calls inside must read the
 * locale at render time, and a const would freeze it at first module load.
 */
function tlsOptions() {
  return [
    { label: m.settings_mail_form_tls_auto(), value: "auto" },
    { label: m.settings_mail_form_tls_starttls(), value: "starttls" },
    { label: m.settings_mail_form_tls_implicit(), value: "implicit" },
    { label: m.settings_mail_form_tls_none(), value: "none" },
  ] as const;
}

const ENCODERS = {
  // SEALED: empty means unchanged, so this key is not sent.
  password: (value: string) => (value === "" ? undefined : value),
  username: (value: string) => (value === "" ? undefined : value),
};

/**
 * Mail settings. An empty `mail.host`, the default, turns mail off.
 *
 * With mail off, password resets and registration confirmations cannot be sent. A
 * single-user instance can run `reset-password` on the machine to recover a forgotten
 * account, but an instance with other users needs a relay.
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
    // SEALED: cannot be read back, so it always starts empty.
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
    tls: asEnum(readSetting(settings, KEYS.tls), MAIL_TLS_MODES, "auto"),
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
  const tls = useWatch({ control: form.control, name: "tls" });

  return (
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      {host.trim() === "" && (
        <SettingsNotice
          icon={<IconInfoCircle />}
          title={m.settings_mail_form_host_empty_title()}
          tone="info"
        >
          {m.settings_mail_form_host_empty_warning()}
        </SettingsNotice>
      )}

      <SettingsCard
        description={m.settings_mail_relay_description()}
        title={m.settings_mail_relay_title()}
      >
        <FieldGroup>
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
            name="tls"
            render={({ field, fieldState }) => (
              <SelectField
                description={m.settings_mail_form_tls_description()}
                field={field}
                fieldState={fieldState}
                id="mail-tls"
                label={m.settings_mail_form_tls_label()}
                options={tlsOptions()}
              />
            )}
          />

          {/*
            `none` means no encryption at all, only for a relay on localhost or the local
            network. The server checks that the resolved address is private or loopback
            and refuses to send otherwise; saying so here avoids finding out at send time.
          */}
          {tls === "none" && (
            <SettingsNotice
              icon={<IconAlertTriangle />}
              title={m.settings_mail_form_tls_none_warning_title()}
              tone="warning"
            >
              {m.settings_mail_form_tls_none_warning_body()}
            </SettingsNotice>
          )}

          <div className="grid gap-4 sm:grid-cols-2">
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
          </div>
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_mail_sender_description()}
        title={m.settings_mail_sender_title()}
      >
        <FieldGroup>
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
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_mail_links_description()}
        title={m.settings_mail_links_title()}
      >
        <FieldGroup>
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
                unit="minutes"
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
