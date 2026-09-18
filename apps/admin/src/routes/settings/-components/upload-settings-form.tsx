import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup, FieldSeparator } from "@tessera/ui/components/field";
import { Controller } from "react-hook-form";

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import { uploadSettingsSchema } from "@/lib/settings-schema";
import type { UploadSettingsValues } from "@/lib/settings-schema";
import { asBoolean, asNumber } from "@/lib/settings-values";

import { SwitchField } from "./form-fields";
import { GuestAnonymousWarning } from "./guest-anonymous-warning";
import { NumberField } from "./number-field";
import { ReadOnlySetting } from "./read-only-setting";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_THROTTLE_WINDOW_SECS = 300;
const DEFAULT_THROTTLE_MAX_ATTEMPTS = 20;
const DEFAULT_DEFAULT_EXPIRY_HOURS = 24 * 7;
const DEFAULT_MAX_EXPIRY_HOURS = 24 * 30;

/** 唯讀顯示用，不進表單 —— 它在 FIXED_AT_BOOT，送出去會被整批退回。 */
const MAX_BODY_BYTES_KEY = "server.max_body_bytes";

const KEYS = {
  allowAnonymousUpload: "features.anonymous_upload",
  defaultExpiryHours: "share.default_expiry_hours",
  maxExpiryHours: "share.max_expiry_hours",
  maxTotalBytes: "storage.max_total_bytes",
  throttleMaxAttempts: "auth.throttle.source.max_attempts",
  throttleWindowSecs: "auth.throttle.source.window_secs",
} satisfies SettingsKeyMap<UploadSettingsValues>;

export function UploadSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <UploadSettingsFields
          defaultValues={initialValues(settings)}
          maxBodyBytes={asNumber(readSetting(settings, MAX_BODY_BYTES_KEY), 0)}
        />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): UploadSettingsValues {
  return {
    allowAnonymousUpload: asBoolean(
      readSetting(settings, KEYS.allowAnonymousUpload),
      true
    ),
    defaultExpiryHours: asNumber(
      readSetting(settings, KEYS.defaultExpiryHours),
      DEFAULT_DEFAULT_EXPIRY_HOURS
    ),
    maxExpiryHours: asNumber(
      readSetting(settings, KEYS.maxExpiryHours),
      DEFAULT_MAX_EXPIRY_HOURS
    ),
    maxTotalBytes: asNumber(readSetting(settings, KEYS.maxTotalBytes), 0),
    throttleMaxAttempts: asNumber(
      readSetting(settings, KEYS.throttleMaxAttempts),
      DEFAULT_THROTTLE_MAX_ATTEMPTS
    ),
    throttleWindowSecs: asNumber(
      readSetting(settings, KEYS.throttleWindowSecs),
      DEFAULT_THROTTLE_WINDOW_SECS
    ),
  };
}

function UploadSettingsFields({
  defaultValues,
  maxBodyBytes,
}: {
  defaultValues: UploadSettingsValues;
  maxBodyBytes: number;
}) {
  const form = useSettingsForm<UploadSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(uploadSettingsSchema),
  });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <GuestAnonymousWarning fixAt="account" />

        <Controller
          control={form.control}
          name="throttleWindowSecs"
          render={({ field, fieldState }) => (
            <NumberField
              description="以來源位址計數的視窗長度，對應 auth.throttle.source.window_secs。這組刻意寬鬆 —— 位址是共用的，擋一個等於擋掉它後面所有人。"
              field={field}
              fieldState={fieldState}
              id="upload-throttle-window"
              label="限流時間窗口（秒）"
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="throttleMaxAttempts"
          render={({ field, fieldState }) => (
            <NumberField
              description="視窗內同一個來源能做幾次昂貴操作。不可為 0 —— 0 次等於鎖死整台機器，server 啟動時就會拒絕。"
              field={field}
              fieldState={fieldState}
              id="upload-throttle-attempts"
              label="允許次數"
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="allowAnonymousUpload"
          render={({ field }) => (
            <SwitchField
              description="關閉後僅登入帳號可以上傳，對應 features.anonymous_upload。"
              field={field}
              id="upload-allow-anonymous"
              label="允許訪客上傳"
            />
          )}
        />

        <ReadOnlySetting
          description={
            <>
              最大請求 body。改了也只有一半會生效 ——
              上傳路徑裡的檢查讀當下設定，但 把關的 middleware
              是建路由時就烤進去的，所以只能改環境變數{" "}
              <code>TESSERA_SERVER__MAX_BODY_BYTES</code> 後重啟。
            </>
          }
          label="單次請求大小上限（bytes）"
          reason="需重啟"
          value={maxBodyBytes.toLocaleString()}
        />

        <FieldSeparator />

        <Controller
          control={form.control}
          name="maxTotalBytes"
          render={({ field, fieldState }) => (
            <NumberField
              description="整個實例的內容總量上限，0 代表不限制。沒設又開著訪客上傳，等於對所有連得到的人開放的無限網路硬碟，而第一個徵兆是磁碟滿了。"
              field={field}
              fieldState={fieldState}
              id="upload-max-total-bytes"
              label="總存儲容量上限（bytes）"
              min={0}
            />
          )}
        />

        <Controller
          control={form.control}
          name="defaultExpiryHours"
          render={({ field, fieldState }) => (
            <NumberField
              description="上傳時未指定保存期限的預設值，0 代表沿用下方的最長保存時間。這比任何配額都更決定磁碟用量。"
              field={field}
              fieldState={fieldState}
              id="upload-default-expiry"
              label="文件預設保存時間（小時）"
              min={0}
            />
          )}
        />

        <Controller
          control={form.control}
          name="maxExpiryHours"
          render={({ field, fieldState }) => (
            <NumberField
              description="使用者可要求的最長保存期限，0 代表沒有上限。"
              field={field}
              fieldState={fieldState}
              id="upload-max-expiry"
              label="文件最長保存時間（小時）"
              min={0}
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
