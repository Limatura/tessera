import { zodResolver } from "@hookform/resolvers/zod";
import { IconAlertTriangle } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Textarea } from "@tessera/ui/components/textarea";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import {
  COOKIE_SECURE_POLICIES,
  networkSettingsSchema,
} from "@/lib/admin/settings-schema";
import type { NetworkSettingsValues } from "@/lib/admin/settings-schema";
import {
  asEnum,
  asLines,
  asNumber,
  asString,
  toLines,
} from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { SelectField, SettingsNotice, TextField } from "./form-fields";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

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

/**
 * 是函式而不是 module-level const —— 裡面呼叫的 `m.xxx()` 要在渲染當下讀 locale，
 * 包成 const 只會在模組第一次載入時凍住語系。
 */
function cookieSecureOptions() {
  return [
    { label: m.settings_network_form_cookie_secure_auto(), value: "auto" },
    { label: m.settings_network_form_cookie_secure_always(), value: "always" },
    { label: m.settings_network_form_cookie_secure_never(), value: "never" },
  ] as const;
}

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
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      <SettingsCard
        description={m.settings_network_address_description()}
        title={m.settings_network_address_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="publicUrl"
            render={({ field, fieldState }) => (
              <TextField
                description={m.settings_network_form_public_url_description()}
                field={field}
                fieldState={fieldState}
                id="network-public-url"
                label={m.settings_network_form_public_url_label()}
                placeholder="https://tessera.example.com"
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_network_proxy_description()}
        title={m.settings_network_proxy_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="trustedProxies"
            render={({ field, fieldState }) => (
              <Field data-invalid={fieldState.invalid}>
                <FieldLabel htmlFor="network-trusted-proxies">
                  {m.settings_network_form_trusted_proxies_label()}
                </FieldLabel>
                <Textarea
                  {...field}
                  aria-invalid={fieldState.invalid}
                  className="font-mono"
                  id="network-trusted-proxies"
                  placeholder={"127.0.0.1\n::1"}
                  rows={3}
                />
                <FieldDescription>
                  {m.settings_network_form_trusted_proxies_description()}
                </FieldDescription>
                {fieldState.invalid && (
                  <FieldError errors={[fieldState.error]} />
                )}
              </Field>
            )}
          />

          <Controller
            control={form.control}
            name="cookieSecure"
            render={({ field, fieldState }) => (
              <SelectField
                description={m.settings_network_form_cookie_secure_description()}
                field={field}
                fieldState={fieldState}
                id="network-cookie-secure"
                label={m.settings_network_form_cookie_secure_label()}
                options={cookieSecureOptions()}
              />
            )}
          />

          {/*
            `always` 在非全 HTTPS 環境會讓登入**靜默**失效：瀏覽器收下回應、丟掉
            cookie，使用者看到「登入成功」然後什麼都沒發生，log 裡也沒有失敗。
          */}
          {cookieSecure === "always" && (
            <SettingsNotice
              icon={<IconAlertTriangle />}
              title={m.settings_network_form_always_secure_title()}
              tone="warning"
            >
              {m.settings_network_form_always_secure_body()}
            </SettingsNotice>
          )}
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_network_timeouts_description()}
        title={m.settings_network_timeouts_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="requestTimeoutSecs"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_network_form_request_timeout_description()}
                field={field}
                fieldState={fieldState}
                id="network-request-timeout"
                label={m.settings_network_form_request_timeout_label()}
                min={1}
                unit="seconds"
              />
            )}
          />

          <Controller
            control={form.control}
            name="uploadStallSecs"
            render={({ field, fieldState }) => (
              <NumberField
                description={m.settings_network_form_upload_stall_description()}
                field={field}
                fieldState={fieldState}
                id="network-upload-stall"
                label={m.settings_network_form_upload_stall_label()}
                min={1}
                unit="seconds"
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsFormFooter form={form} />
    </form>
  );
}
