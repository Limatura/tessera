import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup, FieldSeparator } from "@tessera/ui/components/field";
import type * as React from "react";
import { Controller } from "react-hook-form";

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import { oauthSettingsSchema } from "@/lib/settings-schema";
import type { OauthSettingsValues } from "@/lib/settings-schema";
import {
  asBoolean,
  asScopes,
  asString,
  isSecretSet,
  toScopes,
} from "@/lib/settings-values";

import { SealedField, SwitchField, TextField } from "./form-fields";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];

const ENCODERS = {
  // SEALED：留空代表不改動。
  clientSecret: (value: string) => (value === "" ? undefined : value),
  scopes: (value: string) => toScopes(value),
};

/**
 * GitHub 或 Discord。
 *
 * 這兩家不是 OIDC provider（不發 id_token、也不publish discovery 文件），所以後端
 * 有各自寫死端點的 adapter；但對 operator 來說要設的東西跟 OIDC 那張卡一樣，所以
 * 這裡長得一樣。
 */
export function OauthProviderCard({
  settings,
  provider,
  title,
  description,
  scopesHint,
  baseUrlHint,
}: {
  settings: Settings;
  /** dotted key 的前綴，`auth.github` 或 `auth.discord`。 */
  provider: "auth.github" | "auth.discord";
  title: string;
  description: React.ReactNode;
  scopesHint: string;
  /** 只有 GitHub 用得到 base_url；不給就不畫那一格，也不會送那個 key。 */
  baseUrlHint?: string;
}) {
  const keys = keysFor(provider, baseUrlHint !== undefined);

  return (
    <SettingsCard description={description} title={title}>
      <OauthProviderFields
        baseUrlHint={baseUrlHint}
        defaultValues={initialValues(settings, provider)}
        idPrefix={provider === "auth.github" ? "github" : "discord"}
        keys={keys}
        scopesHint={scopesHint}
        secretIsSet={isSecretSet(settings, `${provider}.client_secret`)}
      />
    </SettingsCard>
  );
}

function keysFor(
  provider: string,
  withBaseUrl: boolean
): SettingsKeyMap<OauthSettingsValues> {
  return {
    // Discord 無視 base_url，那張卡不畫也不送 —— 送一個 provider 根本不看的值，
    // 只會讓下次有人來讀設定時以為它有作用。
    baseUrl: withBaseUrl ? `${provider}.base_url` : undefined,
    clientId: `${provider}.client_id`,
    clientSecret: `${provider}.client_secret`,
    displayName: `${provider}.display_name`,
    linkByVerifiedEmail: `${provider}.link_by_verified_email`,
    scopes: `${provider}.scopes`,
  };
}

function initialValues(
  settings: Settings,
  provider: string
): OauthSettingsValues {
  return {
    baseUrl: asString(readSetting(settings, `${provider}.base_url`), ""),
    clientId: asString(readSetting(settings, `${provider}.client_id`), ""),
    // SEALED：讀不回值，永遠從空的開始。
    clientSecret: "",
    displayName: asString(
      readSetting(settings, `${provider}.display_name`),
      ""
    ),
    linkByVerifiedEmail: asBoolean(
      readSetting(settings, `${provider}.link_by_verified_email`),
      false
    ),
    scopes: asScopes(readSetting(settings, `${provider}.scopes`), ""),
  };
}

function OauthProviderFields({
  defaultValues,
  keys,
  idPrefix,
  scopesHint,
  baseUrlHint,
  secretIsSet,
}: {
  defaultValues: OauthSettingsValues;
  keys: SettingsKeyMap<OauthSettingsValues>;
  idPrefix: string;
  scopesHint: string;
  baseUrlHint?: string;
  secretIsSet: boolean;
}) {
  const form = useSettingsForm<OauthSettingsValues>({
    defaultValues,
    encoders: ENCODERS,
    keys,
    resolver: zodResolver(oauthSettingsSchema),
  });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <Controller
          control={form.control}
          name="clientId"
          render={({ field, fieldState }) => (
            <TextField
              description="填了 Client ID 與 Client Secret 才會出現這個登入按鈕。"
              field={field}
              fieldState={fieldState}
              id={`${idPrefix}-client-id`}
              label="Client ID"
            />
          )}
        />

        <Controller
          control={form.control}
          name="clientSecret"
          render={({ field, fieldState }) => (
            <SealedField
              field={field}
              fieldState={fieldState}
              id={`${idPrefix}-client-secret`}
              isSet={secretIsSet}
              label="Client Secret"
            />
          )}
        />

        <Controller
          control={form.control}
          name="scopes"
          render={({ field, fieldState }) => (
            <TextField
              description={scopesHint}
              field={field}
              fieldState={fieldState}
              id={`${idPrefix}-scopes`}
              label="Scopes"
            />
          )}
        />

        {baseUrlHint !== undefined && (
          <Controller
            control={form.control}
            name="baseUrl"
            render={({ field, fieldState }) => (
              <TextField
                description={baseUrlHint}
                field={field}
                fieldState={fieldState}
                id={`${idPrefix}-base-url`}
                label="自訂網域"
                placeholder="https://github.example.com"
              />
            )}
          />
        )}

        <FieldSeparator />

        <Controller
          control={form.control}
          name="displayName"
          render={({ field, fieldState }) => (
            <TextField
              description="登入按鈕上的文字，留空則用 provider 的名稱。"
              field={field}
              fieldState={fieldState}
              id={`${idPrefix}-display-name`}
              label="按鈕顯示名稱"
            />
          )}
        />

        <Controller
          control={form.control}
          name="linkByVerifiedEmail"
          render={({ field }) => (
            <SwitchField
              description="允許這個身分綁到同 email 的既有帳號。這兩家的帳號一分鐘就能註冊，同時開放註冊又用 email 比對等於把別人的帳號送出去 —— 預設關閉。"
              field={field}
              id={`${idPrefix}-link-by-email`}
              label="用已驗證的 email 綁定既有帳號"
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
