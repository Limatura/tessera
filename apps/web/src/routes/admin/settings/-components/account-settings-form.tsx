import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup } from "@tessera/ui/components/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { Controller, useWatch } from "react-hook-form";

import { useRoles } from "@/hooks/admin/use-admin";
import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import { roleLabel } from "@/lib/admin/permissions";
import {
  REGISTRATION_MODES,
  accountSettingsSchema,
} from "@/lib/admin/settings-schema";
import type { AccountSettingsValues } from "@/lib/admin/settings-schema";
import { asEnum, asNumber, asString } from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { SelectField, SettingRow } from "./form-fields";
import { GuestAnonymousWarning } from "./guest-anonymous-warning";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

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
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      <GuestAnonymousWarning defaultRole={defaultRole} fixAt="upload" />

      <SettingsCard
        description={m.settings_account_signup_description()}
        title={m.settings_account_signup_title()}
      >
        <FieldGroup>
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
                error={fieldState.invalid ? fieldState.error : undefined}
                onChange={field.onChange}
                value={field.value}
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_account_sessions_description()}
        title={m.settings_account_sessions_title()}
      >
        <FieldGroup>
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
                unit="hours"
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

/**
 * 新註冊帳號拿到的角色。
 *
 * 送出的是角色「名稱」不是 id。列表要 `roles:read`，拿不到時仍把目前的值列成唯一選項 ——
 * 不然一存就把設定改成空字串了。
 */
function DefaultRoleField({
  value,
  onChange,
  error,
}: {
  value: string;
  onChange: (value: string) => void;
  error?: { message?: string };
}) {
  const roles = useRoles();
  const names = roles.data?.map((role) => role.name) ?? [];
  const options = names.includes(value) ? names : [value, ...names];

  return (
    <SettingRow
      controlClassName="@md/field-group:w-60"
      description={
        <>
          {m.settings_account_form_default_role_description()}
          {roles.isError && ` ${m.settings_account_form_roles_error()}`}
        </>
      }
      error={error}
      id="account-default-role"
      label={m.settings_account_form_default_role_label()}
    >
      <Select onValueChange={(next) => onChange(next ?? "")} value={value}>
        <SelectTrigger className="w-full" id="account-default-role">
          <SelectValue
            placeholder={m.settings_account_form_default_role_placeholder()}
          >
            {(name: string) => roleLabel(name)}
          </SelectValue>
        </SelectTrigger>
        <SelectContent alignItemWithTrigger={false}>
          <SelectGroup>
            {options.map((name) => (
              <SelectItem key={name} value={name}>
                {roleLabel(name)}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </SettingRow>
  );
}
