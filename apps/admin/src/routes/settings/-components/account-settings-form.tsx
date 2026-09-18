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

const REGISTRATION_OPTIONS = [
  { label: "任何人都能註冊", value: "open" },
  { label: "需要邀請碼", value: "invite" },
  { label: "關閉註冊", value: "closed" },
] as const;

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
              description="關閉註冊後仍可由管理者手動建立帳號。"
              field={field}
              fieldState={fieldState}
              id="account-registration"
              label="註冊方式"
              options={REGISTRATION_OPTIONS}
              placeholder="選擇註冊方式"
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
              description="使用者登入後多久需要重新登入，預設 720 小時（30 天）。"
              field={field}
              fieldState={fieldState}
              id="account-session-ttl"
              label="登入會話時長（小時）"
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="loginHistoryDays"
          render={({ field, fieldState }) => (
            <NumberField
              description="已結束的 session 在登入紀錄留幾天，預設 90 天。用途是回答「有沒有別人進來過」，所以活得比憑證久。"
              field={field}
              fieldState={fieldState}
              id="account-login-history-days"
              label="登入紀錄保留天數"
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
      <FieldLabel htmlFor="account-default-role">新帳號預設角色</FieldLabel>
      <Select onValueChange={(next) => onChange(next ?? "")} value={value}>
        <SelectTrigger id="account-default-role">
          <SelectValue placeholder="選擇角色" />
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
        註冊完成時指派的角色，對應 auth.default_role。
        {roles.isError && "（角色列表載入失敗，只能沿用目前的值）"}
      </FieldDescription>
    </Field>
  );
}
