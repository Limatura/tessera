import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup } from "@tessera/ui/components/field";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import { passwordSettingsSchema } from "@/lib/admin/settings-schema";
import type { PasswordSettingsValues } from "@/lib/admin/settings-schema";
import { asBoolean, asNumber, asString } from "@/lib/admin/settings-values";
import {
  PASSWORD_MAX_CEILING,
  PASSWORD_MIN_FLOOR,
} from "@/lib/api/password-policy";
import * as m from "@/paraglide/messages";

import { SwitchField, TextField } from "./form-fields";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_MIN_LENGTH = 8;
const DEFAULT_MAX_LENGTH = 256;
const DEFAULT_MEMORY_KIB = 19_456;
const DEFAULT_ITERATIONS = 2;
const DEFAULT_LANES = 1;
const DEFAULT_PWNED_URL = "https://api.pwnedpasswords.com";

const KEYS = {
  iterations: "auth.hashing.iterations",
  lanes: "auth.hashing.lanes",
  maxLength: "auth.password.max_length",
  memoryKib: "auth.hashing.memory_kib",
  minLength: "auth.password.min_length",
  pwnedEnabled: "auth.pwned.enabled",
  pwnedUrl: "auth.pwned.url",
  requireDigit: "auth.password.require_digit",
  requireMixedCase: "auth.password.require_mixed_case",
  requireSymbol: "auth.password.require_symbol",
} satisfies SettingsKeyMap<PasswordSettingsValues>;

export function PasswordSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <PasswordSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): PasswordSettingsValues {
  return {
    iterations: asNumber(
      readSetting(settings, KEYS.iterations),
      DEFAULT_ITERATIONS
    ),
    lanes: asNumber(readSetting(settings, KEYS.lanes), DEFAULT_LANES),
    maxLength: asNumber(
      readSetting(settings, KEYS.maxLength),
      DEFAULT_MAX_LENGTH
    ),
    memoryKib: asNumber(
      readSetting(settings, KEYS.memoryKib),
      DEFAULT_MEMORY_KIB
    ),
    minLength: asNumber(
      readSetting(settings, KEYS.minLength),
      DEFAULT_MIN_LENGTH
    ),
    pwnedEnabled: asBoolean(readSetting(settings, KEYS.pwnedEnabled), false),
    pwnedUrl: asString(readSetting(settings, KEYS.pwnedUrl), DEFAULT_PWNED_URL),
    requireDigit: asBoolean(readSetting(settings, KEYS.requireDigit), false),
    requireMixedCase: asBoolean(
      readSetting(settings, KEYS.requireMixedCase),
      false
    ),
    requireSymbol: asBoolean(readSetting(settings, KEYS.requireSymbol), false),
  };
}

function PasswordSettingsFields({
  defaultValues,
}: {
  defaultValues: PasswordSettingsValues;
}) {
  const form = useSettingsForm<PasswordSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(passwordSettingsSchema),
  });

  const pwnedEnabled = useWatch({
    control: form.control,
    name: "pwnedEnabled",
  });

  return (
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      <SettingsCard
        description={m.settings_password_length_description()}
        title={m.settings_password_length_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="minLength"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_password_form_min_length_description()}
                field={field}
                fieldState={fieldState}
                id="password-min-length"
                label={m.settings_password_form_min_length_label()}
                max={PASSWORD_MAX_CEILING}
                min={PASSWORD_MIN_FLOOR}
              />
            )}
          />

          <Controller
            control={form.control}
            name="maxLength"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_password_form_max_length_description()}
                field={field}
                fieldState={fieldState}
                id="password-max-length"
                label={m.settings_password_form_max_length_label()}
                max={PASSWORD_MAX_CEILING}
                min={PASSWORD_MIN_FLOOR}
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_password_composition_description()}
        title={m.settings_password_composition_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="requireMixedCase"
            render={({ field }) => (
              <SwitchField
                field={field}
                id="password-require-mixed-case"
                label={m.settings_password_form_require_mixed_case_label()}
              />
            )}
          />

          <Controller
            control={form.control}
            name="requireDigit"
            render={({ field }) => (
              <SwitchField
                field={field}
                id="password-require-digit"
                label={m.settings_password_form_require_digit_label()}
              />
            )}
          />

          <Controller
            control={form.control}
            name="requireSymbol"
            render={({ field }) => (
              <SwitchField
                description={m.settings_password_form_require_symbol_description()}
                field={field}
                id="password-require-symbol"
                label={m.settings_password_form_require_symbol_label()}
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_password_pwned_description()}
        title={m.settings_password_pwned_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="pwnedEnabled"
            render={({ field }) => (
              <SwitchField
                description={m.settings_password_form_pwned_enabled_description()}
                field={field}
                id="pwned-enabled"
                label={m.settings_password_form_pwned_enabled_label()}
              />
            )}
          />

          {pwnedEnabled && (
            <Controller
              control={form.control}
              name="pwnedUrl"
              render={({ field, fieldState }) => (
                <TextField
                  description={m.settings_password_form_pwned_url_description()}
                  field={field}
                  fieldState={fieldState}
                  id="pwned-url"
                  label={m.settings_password_form_pwned_url_label()}
                  placeholder={DEFAULT_PWNED_URL}
                />
              )}
            />
          )}
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_password_hashing_description()}
        title={m.settings_password_hashing_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="memoryKib"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_password_form_memory_kib_description()}
                field={field}
                fieldState={fieldState}
                id="hashing-memory-kib"
                label={m.settings_password_form_memory_kib_label()}
                min={8}
                unit="kib"
              />
            )}
          />

          <Controller
            control={form.control}
            name="iterations"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_password_form_iterations_description()}
                field={field}
                fieldState={fieldState}
                id="hashing-iterations"
                label={m.settings_password_form_iterations_label()}
                min={1}
              />
            )}
          />

          <Controller
            control={form.control}
            name="lanes"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_password_form_lanes_description()}
                field={field}
                fieldState={fieldState}
                id="hashing-lanes"
                label={m.settings_password_form_lanes_label()}
                min={1}
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsFormFooter form={form} />
    </form>
  );
}
