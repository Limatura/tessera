import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldError,
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
import { Controller, useForm, useWatch } from "react-hook-form";

import { useRoles } from "@/hooks/use-admin";
import { useSession } from "@/hooks/use-auth";
import {
  readSetting,
  useSettings,
  useUpdateSettings,
} from "@/hooks/use-settings";
import { PERMISSIONS, hasPermission, roleLabel } from "@/lib/permissions";
import {
  REGISTRATION_MODES,
  accountSettingsSchema,
} from "@/lib/settings-schema";
import type { AccountSettingsValues } from "@/lib/settings-schema";

import { GuestAnonymousWarning } from "./guest-anonymous-warning";
import { NumberField } from "./number-field";

type ErrorBody = components["schemas"]["ErrorBody"];

/** 表單欄位 ↔ server 的 dotted key。讀初始值跟送出都走這張表。 */
const KEYS: Record<keyof AccountSettingsValues, string> = {
  defaultRole: "auth.default_role",
  loginHistoryDays: "auth.login_history_days",
  registration: "auth.registration",
  sessionTtlHours: "auth.session_ttl_hours",
};

const FALLBACKS: AccountSettingsValues = {
  defaultRole: "member",
  loginHistoryDays: 90,
  registration: "open",
  sessionTtlHours: 24 * 30,
};

const REGISTRATION_LABELS: Record<
  AccountSettingsValues["registration"],
  string
> = {
  closed: "關閉註冊",
  invite: "需要邀請碼",
  open: "任何人都能註冊",
};

function asString(value: unknown, fallback: string): string {
  return typeof value === "string" ? value : fallback;
}

function asNumber(value: unknown, fallback: number): number {
  return typeof value === "number" ? value : fallback;
}

function asRegistration(value: unknown): AccountSettingsValues["registration"] {
  return (
    REGISTRATION_MODES.find((mode) => mode === value) ?? FALLBACKS.registration
  );
}

type Settings = components["schemas"]["SettingsResponse"];

function initialValues(settings: Settings): AccountSettingsValues {
  return {
    defaultRole: asString(
      readSetting(settings, KEYS.defaultRole),
      FALLBACKS.defaultRole
    ),
    loginHistoryDays: asNumber(
      readSetting(settings, KEYS.loginHistoryDays),
      FALLBACKS.loginHistoryDays
    ),
    registration: asRegistration(readSetting(settings, KEYS.registration)),
    sessionTtlHours: asNumber(
      readSetting(settings, KEYS.sessionTtlHours),
      FALLBACKS.sessionTtlHours
    ),
  };
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

function AccountSettingsFields({
  defaultValues,
}: {
  defaultValues: AccountSettingsValues;
}) {
  const { data: account } = useSession();
  const update = useUpdateSettings();
  const {
    control,
    handleSubmit,
    reset,
    formState: { dirtyFields, isDirty },
  } = useForm<AccountSettingsValues>({
    defaultValues,
    resolver: zodResolver(accountSettingsSchema),
  });

  const defaultRole = useWatch({ control, name: "defaultRole" });
  const canWrite = hasPermission(account, PERMISSIONS.settingsWrite);
  const error = update.error as ErrorBody | null;

  // 只送改過的欄位。這一頁的四個都可以改，但同一批裡混進一個 server 拒絕的 key
  // 會讓整批退回，沒動過的就別送。
  const onSubmit = handleSubmit((values) => {
    const changes: Record<string, unknown> = {};
    for (const field of Object.keys(KEYS) as (keyof AccountSettingsValues)[]) {
      if (dirtyFields[field]) {
        changes[KEYS[field]] = values[field];
      }
    }

    if (Object.keys(changes).length === 0) {
      return;
    }

    update.mutate(changes, { onSuccess: () => reset(values) });
  });

  return (
    <form onSubmit={onSubmit}>
      <FieldGroup>
        <GuestAnonymousWarning defaultRole={defaultRole} fixAt="upload" />

        <Controller
          control={control}
          name="registration"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="account-registration">註冊方式</FieldLabel>
              <Select onValueChange={field.onChange} value={field.value}>
                <SelectTrigger id="account-registration">
                  <SelectValue placeholder="選擇註冊方式" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {REGISTRATION_MODES.map((mode) => (
                      <SelectItem key={mode} value={mode}>
                        {REGISTRATION_LABELS[mode]}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
              <FieldDescription>
                關閉註冊後仍可由管理者手動建立帳號。
              </FieldDescription>
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />

        <Controller
          control={control}
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
          control={control}
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
          control={control}
          name="loginHistoryDays"
          render={({ field, fieldState }) => (
            <NumberField
              description="登入紀錄保留幾天後清除，預設 90 天。"
              field={field}
              fieldState={fieldState}
              id="account-login-history-days"
              label="登入紀錄保留天數"
              min={1}
            />
          )}
        />

        {!canWrite && (
          <p className="text-muted-foreground text-sm">
            你只能檢視這些設定，變更需要 settings:write 權限。
          </p>
        )}

        {error && (
          <p className="text-destructive text-sm" role="alert">
            {error.message ?? "儲存失敗"}
          </p>
        )}

        <Field orientation="horizontal">
          <Button
            disabled={!(canWrite && isDirty) || update.isPending}
            type="submit"
          >
            {update.isPending ? "儲存中…" : "儲存"}
          </Button>
          {update.isSuccess && !isDirty && (
            <span className="text-muted-foreground text-sm">已儲存</span>
          )}
        </Field>
      </FieldGroup>
    </form>
  );
}

export function AccountSettingsForm() {
  const settings = useSettings();

  if (settings.isPending) {
    return <p className="text-muted-foreground text-sm">載入中…</p>;
  }

  if (settings.isError || !settings.data) {
    return <p className="text-destructive text-sm">設定載入失敗</p>;
  }

  // 初始值只在掛載時讀一次 —— 背景重抓不該把使用者打到一半的內容洗掉。
  return <AccountSettingsFields defaultValues={initialValues(settings.data)} />;
}
