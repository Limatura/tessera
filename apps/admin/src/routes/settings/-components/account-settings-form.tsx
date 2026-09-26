import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { Controller, useWatch } from "react-hook-form";

import { useRoles } from "@/hooks/use-admin";
import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import { roleLabel } from "@/lib/permissions";
import {
  REGISTRATION_MODES,
  accountSettingsSchema,
} from "@/lib/settings-schema";
import type { AccountSettingsValues } from "@/lib/settings-schema";
import { asEnum, asNumber, asString } from "@/lib/settings-values";
import * as m from "@/paraglide/messages";

import { SelectField } from "./form-fields";
import { GuestAnonymousWarning } from "./guest-anonymous-warning";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_SESSION_TTL_HOURS = 24 * 30;
const DEFAULT_LOGIN_HISTORY_DAYS = 90;

const KEYS = {
  defaultRole: "auth.default_role",
  loginHistoryDays: "auth.login_history_days",
  registration: "auth.registration",
  sessionTtlHours: "auth.session_ttl_hours",
} satisfies SettingsKeyMap<AccountSettingsValues>;

/**
 * 是函式而不是 module-level const —— 裡面呼叫的 `m.xxx()` 要在渲染當下讀 locale，
 * 包成 const 只會在模組第一次載入時凍住語系。
 */
function registrationOptions() {
  return [
    { label: m.settings_account_form_registration_open(), value: "open" },
    { label: m.settings_account_form_registration_invite(), value: "invite" },
    { label: m.settings_account_form_registration_closed(), value: "closed" },
  ] as const;
}

export function AccountSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <AccountSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): AccountSettingsValues {
  return {
    defaultRole: asString(readSetting(settings, KEYS.defaultRole), "member"),
    loginHistoryDays: asNumber(
      readSetting(settings, KEYS.loginHistoryDays),
      DEFAULT_LOGIN_HISTORY_DAYS
    ),
    registration: asEnum(
      readSetting(settings, KEYS.registration),
      REGISTRATION_MODES,
      "open"
    ),
    sessionTtlHours: asNumber(
      readSetting(settings, KEYS.sessionTtlHours),
      DEFAULT_SESSION_TTL_HOURS
    ),
  };
}

function AccountSettingsFields({
  defaultValues,
}: {
  defaultValues: AccountSettingsValues;
}) {
  const form = useSettingsForm<AccountSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(accountSettingsSchema),
  });

  const defaultRole = useWatch({ control: form.control, name: "defaultRole" });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <GuestAnonymousWarning defaultRole={defaultRole} fixAt="upload" />

        <Controller
          control={form.control}
          name="registration"
          render={({ field, fieldState }) => (
            <SelectField
              description={m.settings_account_form_registration_description()}
              field={field}
              fieldState={fieldState}
              id="account-registration"
              label={m.settings_account_form_registration_label()}
              options={registrationOptions()}
              placeholder={m.settings_account_form_registration_placeholder()}
            />
          )}
        />

        <Controller
          control={form.control}
          name="defaultRole"
          render={({ field, fieldState }) => (
            <DefaultRoleField
              invalid={fieldState.invalid}
              onChange={field.onChange}
              value={field.value}
            />
          )}
        />

        <Controller
          control={form.control}
          name="sessionTtlHours"
          render={({ field, fieldState }) => (
            <NumberField
              description={m.settings_account_form_session_ttl_description()}
              field={field}
              fieldState={fieldState}
              id="account-session-ttl"
              label={m.settings_account_form_session_ttl_label()}
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="loginHistoryDays"
          render={({ field, fieldState }) => (
            <NumberField
              description={m.settings_account_form_login_history_description()}
              field={field}
              fieldState={fieldState}
              id="account-login-history-days"
              label={m.settings_account_form_login_history_label()}
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

/**
 * 新註冊帳號拿到的角色。
 *
 * 送出的是角色「名稱」不是 id。列表要 `roles:read`，拿不到時仍把目前的值列成唯一選項 ——
 * 不然一存就把設定改成空字串了。
 */
function DefaultRoleField({
  value,
  onChange,
  invalid,
}: {
  value: string;
  onChange: (value: string) => void;
  invalid: boolean;
}) {
  const roles = useRoles();
  const names = roles.data?.map((role) => role.name) ?? [];
  const options = names.includes(value) ? names : [value, ...names];

  return (
    <Field data-invalid={invalid}>
      <FieldLabel htmlFor="account-default-role">
        {m.settings_account_form_default_role_label()}
      </FieldLabel>
      <Select onValueChange={(next) => onChange(next ?? "")} value={value}>
        <SelectTrigger id="account-default-role">
          <SelectValue
            placeholder={m.settings_account_form_default_role_placeholder()}
          />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {options.map((name) => (
              <SelectItem key={name} value={name}>
                {roleLabel(name)}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <FieldDescription>
        {m.settings_account_form_default_role_description()}
        {roles.isError && m.settings_account_form_roles_error()}
      </FieldDescription>
    </Field>
  );
}
