import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
} from "@tessera/ui/components/field";
import { Textarea } from "@tessera/ui/components/textarea";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import {
  COOKIE_SECURE_POLICIES,
  networkSettingsSchema,
} from "@/lib/settings-schema";
import type { NetworkSettingsValues } from "@/lib/settings-schema";
import {
  asEnum,
  asLines,
  asNumber,
  asString,
  toLines,
} from "@/lib/settings-values";

import { SelectField, TextField } from "./form-fields";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_REQUEST_TIMEOUT_SECS = 30;
const DEFAULT_UPLOAD_STALL_SECS = 30;

const KEYS = {
  cookieSecure: "auth.cookie_secure",
  publicUrl: "server.public_url",
  requestTimeoutSecs: "server.request_timeout_secs",
  trustedProxies: "server.trusted_proxies",
  uploadStallSecs: "server.upload_stall_secs",
} satisfies SettingsKeyMap<NetworkSettingsValues>;

const ENCODERS = {
  trustedProxies: (value: string) => toLines(value),
};

const COOKIE_SECURE_OPTIONS = [
  { label: "自動（走 HTTPS 才標，建議）", value: "auto" },
  { label: "總是標記 Secure", value: "always" },
  { label: "永不標記", value: "never" },
] as const;

export function NetworkSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <NetworkSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): NetworkSettingsValues {
  return {
    cookieSecure: asEnum(
      readSetting(settings, KEYS.cookieSecure),
      COOKIE_SECURE_POLICIES,
      "auto"
    ),
    publicUrl: asString(readSetting(settings, KEYS.publicUrl), ""),
    requestTimeoutSecs: asNumber(
      readSetting(settings, KEYS.requestTimeoutSecs),
      DEFAULT_REQUEST_TIMEOUT_SECS
    ),
    trustedProxies: asLines(readSetting(settings, KEYS.trustedProxies), ""),
    uploadStallSecs: asNumber(
      readSetting(settings, KEYS.uploadStallSecs),
      DEFAULT_UPLOAD_STALL_SECS
    ),
  };
}

function NetworkSettingsFields({
  defaultValues,
}: {
  defaultValues: NetworkSettingsValues;
}) {
  const form = useSettingsForm<NetworkSettingsValues>({
    defaultValues,
    encoders: ENCODERS,
    keys: KEYS,
    resolver: zodResolver(networkSettingsSchema),
  });

  const cookieSecure = useWatch({
    control: form.control,
    name: "cookieSecure",
  });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <Controller
          control={form.control}
          name="publicUrl"
          render={({ field, fieldState }) => (
            <TextField
              description="本站對外的絕對 URL。只有連結預覽需要它，而且必須絕對 —— og:image 是別台機器上的爬蟲來抓的。留空則從每個請求的 Host 推導，那在會改寫 Host 的反向代理、或只看得到 localhost 的容器裡會壞掉。預覽卡片一片空白就是要設這個。"
              field={field}
              fieldState={fieldState}
              id="network-public-url"
              label="對外網址"
              placeholder="https://tessera.example.com"
            />
          )}
        />

        <Controller
          control={form.control}
          name="trustedProxies"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="network-trusted-proxies">
                可信任的前置代理
              </FieldLabel>
              <Textarea
                {...field}
                aria-invalid={fieldState.invalid}
                id="network-trusted-proxies"
                placeholder={"127.0.0.1\n::1"}
                rows={3}
              />
              <FieldDescription>
                一行一筆，CIDR 或裸位址。同機代理填 127.0.0.1 與 ::1，容器網路填
                10.0.0.0/8。預設什麼都不信是刻意的：X-Forwarded-For
                是最後跟我們講話的人 設的，無條件相信等於讓呼叫者自選來源位址 ——
                而下面每一組限流都以那個位址計數。
              </FieldDescription>
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />

        <Controller
          control={form.control}
          name="cookieSecure"
          render={({ field, fieldState }) => (
            <SelectField
              description="何時給 session cookie 標 Secure。自動會在走 HTTPS 時才標，同時相容 LAN 明文與公開 TLS 兩種部署。"
              field={field}
              fieldState={fieldState}
              id="network-cookie-secure"
              label="Session cookie 的 Secure 標記"
              options={COOKIE_SECURE_OPTIONS}
            />
          )}
        />

        {cookieSecure === "always" && <AlwaysSecureWarning />}

        <FieldSeparator />

        <Controller
          control={form.control}
          name="requestTimeoutSecs"
          render={({ field, fieldState }) => (
            <NumberField
              description="一般請求的時鐘預算，預設 30 秒。不套用在上傳路由 —— 一個數字沒辦法同時滿足「5GB 檔案慢慢傳」跟「連上來什麼都不送的人別佔位置」。"
              field={field}
              fieldState={fieldState}
              id="network-request-timeout"
              label="請求逾時（秒）"
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="uploadStallSecs"
          render={({ field, fieldState }) => (
            <NumberField
              description="上傳多久沒有位元組進來就算斷線，預設 30 秒。問的是「對面還在送嗎」，所以大檔要傳多久都不受影響。"
              field={field}
              fieldState={fieldState}
              id="network-upload-stall"
              label="上傳停滯逾時（秒）"
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
 * `always` 在非全 HTTPS 環境會讓登入**靜默**失效，這是這一頁最容易踩的坑。
 *
 * 瀏覽器會收下回應、丟掉 cookie，所以使用者看到「登入成功」然後什麼都沒發生 ——
 * 沒有錯誤訊息、log 裡也沒有失敗。
 */
function AlwaysSecureWarning() {
  return (
    <div
      className="flex flex-col gap-1 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm"
      role="alert"
    >
      <p className="font-medium">只有全程 HTTPS 的部署才該選這個</p>
      <p className="text-muted-foreground">
        在有任何一段是明文 HTTP 的環境下，瀏覽器會收下回應卻丟掉 cookie ——
        使用者看到登入成功，然後什麼都沒發生，而且不會有錯誤訊息。不確定的話選「自動」。
      </p>
    </div>
  );
}
