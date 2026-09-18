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
            SMTP 主機留空代表這個實例不寄任何信，重設密碼與註冊確認都不會運作。
            這裡預期的是 relay（Resend、Brevo、SES、Fastmail 或公司自己的）——
            家用線路 送不出信，多數 ISP 封鎖 port 25，收信端也直接拒絕住宅位址。
          </p>
        )}

        <Controller
          control={form.control}
          name="host"
          render={({ field, fieldState }) => (
            <TextField
              description="SMTP 主機。留空關閉所有寄信功能。"
              field={field}
              fieldState={fieldState}
              id="mail-host"
              label="SMTP 主機"
              placeholder="smtp.resend.com"
            />
          )}
        />

        <Controller
          control={form.control}
          name="port"
          render={({ field, fieldState }) => (
            <NumberField
              description="587 是 submission + STARTTLS（預設）；465 是隱式 TLS，也常見。"
              field={field}
              fieldState={fieldState}
              id="mail-port"
              label="SMTP 連接埠"
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
              description="Resend 要求這裡填字面字串 resend。"
              field={field}
              fieldState={fieldState}
              id="mail-username"
              isSet={usernameIsSet}
              label="SMTP 帳號"
            />
          )}
        />

        <Controller
          control={form.control}
          name="password"
          render={({ field, fieldState }) => (
            <SealedField
              description="多數 relay 這裡放的是 API key。"
              field={field}
              fieldState={fieldState}
              id="mail-password"
              isSet={passwordIsSet}
              label="SMTP 密碼"
            />
          )}
        />

        <FieldSeparator />

        <Controller
          control={form.control}
          name="from"
          render={({ field, fieldState }) => (
            <TextField
              description="信件的 From: 位址。網域必須已經在 relay 那邊驗證過，否則會被退信。"
              field={field}
              fieldState={fieldState}
              id="mail-from"
              label="寄件位址"
              placeholder="tessera@example.com"
            />
          )}
        />

        <Controller
          control={form.control}
          name="publicUrl"
          render={({ field, fieldState }) => (
            <TextField
              description="信件內連結的基底。刻意用設定值而非請求的 Host header —— 用可被偽造的 header 組出的重設連結會指向攻擊者的伺服器，等於把 token 送給他。"
              field={field}
              fieldState={fieldState}
              id="mail-public-url"
              label="信件連結基底"
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
              description="重設密碼連結的有效分鐘數，預設 30，上限 1440（24 小時）。短是因為這個連結本身就是憑證，夠久撐過郵件佇列即可。"
              field={field}
              fieldState={fieldState}
              id="mail-reset-ttl"
              label="重設密碼連結時效（分鐘）"
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
              description="註冊確認連結的時效，預設 24，上限 168（7 天）。可以長是因為它給的少 —— 點了之後建立的帳號用的是申請者自己早就選好的密碼。"
              field={field}
              fieldState={fieldState}
              id="mail-registration-ttl"
              label="註冊確認連結時效（小時）"
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
