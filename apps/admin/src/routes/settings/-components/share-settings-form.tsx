import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup, FieldSeparator } from "@tessera/ui/components/field";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import {
  CODE_LENGTH_MAX,
  CODE_LENGTH_MIN,
  shareSettingsSchema,
} from "@/lib/settings-schema";
import type { ShareSettingsValues } from "@/lib/settings-schema";
import { asBoolean, asNumber } from "@/lib/settings-values";

import { SwitchField } from "./form-fields";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_CODE_LENGTH = 8;
const DEFAULT_ACCESS_LOG_RETENTION_DAYS = 30;
const DEFAULT_INLINE_THRESHOLD_BYTES = 64 * 1024;
const DEFAULT_MAX_TEXT_BYTES = 1024 * 1024;
const DEFAULT_TICKET_TTL_HOURS = 24;
const DEFAULT_TICKET_IDLE_HOURS = 1;
const DEFAULT_RETAIN_AFTER_DEATH_HOURS = 24 * 7;
const DEFAULT_UPLOAD_SESSION_TTL_HOURS = 24;

const KEYS = {
  accessLogEnabled: "share.access_log.enabled",
  accessLogRetentionDays: "share.access_log.retention_days",
  codeLength: "share.code_length",
  inlineThresholdBytes: "share.inline_threshold_bytes",
  maxTextBytes: "share.max_text_bytes",
  retainAfterDeathHours: "share.retain_after_death_hours",
  ticketIdleHours: "share.ticket_idle_hours",
  ticketTtlHours: "share.ticket_ttl_hours",
  uploadSessionTtlHours: "share.upload_session_ttl_hours",
} satisfies SettingsKeyMap<ShareSettingsValues>;

export function ShareSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <ShareSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): ShareSettingsValues {
  return {
    accessLogEnabled: asBoolean(
      readSetting(settings, KEYS.accessLogEnabled),
      false
    ),
    accessLogRetentionDays: asNumber(
      readSetting(settings, KEYS.accessLogRetentionDays),
      DEFAULT_ACCESS_LOG_RETENTION_DAYS
    ),
    codeLength: asNumber(
      readSetting(settings, KEYS.codeLength),
      DEFAULT_CODE_LENGTH
    ),
    inlineThresholdBytes: asNumber(
      readSetting(settings, KEYS.inlineThresholdBytes),
      DEFAULT_INLINE_THRESHOLD_BYTES
    ),
    maxTextBytes: asNumber(
      readSetting(settings, KEYS.maxTextBytes),
      DEFAULT_MAX_TEXT_BYTES
    ),
    retainAfterDeathHours: asNumber(
      readSetting(settings, KEYS.retainAfterDeathHours),
      DEFAULT_RETAIN_AFTER_DEATH_HOURS
    ),
    ticketIdleHours: asNumber(
      readSetting(settings, KEYS.ticketIdleHours),
      DEFAULT_TICKET_IDLE_HOURS
    ),
    ticketTtlHours: asNumber(
      readSetting(settings, KEYS.ticketTtlHours),
      DEFAULT_TICKET_TTL_HOURS
    ),
    uploadSessionTtlHours: asNumber(
      readSetting(settings, KEYS.uploadSessionTtlHours),
      DEFAULT_UPLOAD_SESSION_TTL_HOURS
    ),
  };
}

function ShareSettingsFields({
  defaultValues,
}: {
  defaultValues: ShareSettingsValues;
}) {
  const form = useSettingsForm<ShareSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(shareSettingsSchema),
  });

  const accessLogEnabled = useWatch({
    control: form.control,
    name: "accessLogEnabled",
  });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <Controller
          control={form.control}
          name="codeLength"
          render={({ field, fieldState }) => (
            <NumberField
              description="取件碼字數，預設 8。8 個 Crockford base32 約是 1.1×10¹² 組合；某些同類服務用的 5 碼只有 3300 萬，公開端點可以被走訪完。短碼只有在「取件碼查詢」那組限流存在時才站得住腳。"
              field={field}
              fieldState={fieldState}
              id="share-code-length"
              label="取件碼長度"
              max={CODE_LENGTH_MAX}
              min={CODE_LENGTH_MIN}
            />
          )}
        />

        <Controller
          control={form.control}
          name="maxTextBytes"
          render={({ field, fieldState }) => (
            <NumberField
              description="純文字分享的上限，預設 1 MiB。文字存在資料庫，所以這同時是單一 row 的最大值，也是一次上傳能佔的最大記憶體 —— 整包收下的東西沒辦法串流。"
              field={field}
              fieldState={fieldState}
              id="share-max-text-bytes"
              label="文字分享上限（bytes）"
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="inlineThresholdBytes"
          render={({ field, fieldState }) => (
            <NumberField
              description="小於此值存資料庫而非物件儲存，預設 64 KiB。多數分享是短筆記或程式片段，跑一趟物件儲存的成本高於省下的空間；順帶讓資料庫備份也涵蓋內容。"
              field={field}
              fieldState={fieldState}
              id="share-inline-threshold"
              label="行內儲存門檻（bytes）"
              min={0}
            />
          )}
        />

        <FieldSeparator />

        <Controller
          control={form.control}
          name="ticketTtlHours"
          render={({ field, fieldState }) => (
            <NumberField
              description="下載授權的存活上限，預設 24 小時。一次授權消耗一次取件次數，之後產生的 ticket 可重複使用（Range 請求、續傳、斷線重試）—— 沒有這層分離，一次中斷的下載就會燒掉一個一次性分享。這是天花板不是常態。"
              field={field}
              fieldState={fieldState}
              id="share-ticket-ttl"
              label="下載授權存活上限（小時）"
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="ticketIdleHours"
          render={({ field, fieldState }) => (
            <NumberField
              description="ticket 閒置多久失效，預設 1 小時，每次使用都往後推。用閒置而非固定壽命：ticket 是一串字所以可轉發，固定壽命必須涵蓋「最慢的線下載最大的檔」，而活那麼久的 ticket 就值得被轉發了。"
              field={field}
              fieldState={fieldState}
              id="share-ticket-idle"
              label="下載授權閒置失效（小時）"
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="uploadSessionTtlHours"
          render={({ field, fieldState }) => (
            <NumberField
              description="未完成的上傳留多久，預設 24 小時。期間它佔的位元組一直算在配額裡。夠久讓闔上的筆電隔夜還能續傳，夠短讓死掉的 session 不堆積。用 S3 的話另外設一條 AbortIncompleteMultipartUpload，天數比這個長幾天。"
              field={field}
              fieldState={fieldState}
              id="share-upload-session-ttl"
              label="未完成上傳保留（小時）"
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="retainAfterDeathHours"
          render={({ field, fieldState }) => (
            <NumberField
              description="過期或用完後保留多久才真刪，預設 168（7 天），0 代表下次清掃就刪。留著是為了讓連結能說「這個過期了」而不是「查無此碼」—— 只有前者值得重打一次。"
              field={field}
              fieldState={fieldState}
              id="share-retain-after-death"
              label="失效後保留（小時）"
              min={0}
            />
          )}
        />

        <FieldSeparator />

        <Controller
          control={form.control}
          name="accessLogEnabled"
          render={({ field }) => (
            <SwitchField
              description="是否逐筆記錄每次取件。預設關閉是刻意的 —— view_count 是個整數，不說明訪客在哪；而一筆存取紀錄是一個位址加一串瀏覽器字串，記的是「收到連結、什麼都沒答應過」的人。"
              field={field}
              id="share-access-log-enabled"
              label="記錄取件紀錄"
            />
          )}
        />

        {accessLogEnabled && (
          <p className="rounded-md border border-border bg-muted/40 p-3 text-muted-foreground text-sm">
            開啟後這個實例會留下訪客的位址與瀏覽器字串。
            <strong className="font-medium text-foreground">
              開啟的實例應該在頁面上說明這件事。
            </strong>
          </p>
        )}

        <Controller
          control={form.control}
          name="accessLogRetentionDays"
          render={({ field, fieldState }) => (
            <NumberField
              description="保留天數，預設 30。開啟時不可為 0 —— 把 0 解讀為「永久」對一個關於別人資料的設定是錯的預設，所以後端直接拒絕。"
              field={field}
              fieldState={fieldState}
              id="share-access-log-retention"
              label="取件紀錄保留天數"
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
