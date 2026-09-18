import { zodResolver } from "@hookform/resolvers/zod";
import { IconAlertTriangle } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { FieldGroup, FieldSeparator } from "@tessera/ui/components/field";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import { oidcSettingsSchema } from "@/lib/settings-schema";
import type { OidcSettingsValues } from "@/lib/settings-schema";
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

const KEYS = {
  clientId: "auth.oidc.client_id",
  clientSecret: "auth.oidc.client_secret",
  displayName: "auth.oidc.display_name",
  groupClaim: "auth.oidc.group_claim",
  issuer: "auth.oidc.issuer",
  linkByVerifiedEmail: "auth.oidc.link_by_verified_email",
  requiredGroup: "auth.oidc.required_group",
  scopes: "auth.oidc.scopes",
} satisfies SettingsKeyMap<OidcSettingsValues>;

const ENCODERS = {
  // SEALED：留空代表不改動，所以連 key 都不要進這一批。
  clientSecret: (value: string) => (value === "" ? undefined : value),
  scopes: (value: string) => toScopes(value),
};

/**
 * 任何標準 OIDC provider（Authentik、Authelia、Keycloak、Google...）。
 *
 * 沒有「啟用」開關 —— 空的 `issuer` 就是關閉，多一個布林只會多出「填了 issuer 但
 * 忘了打開」這種看起來設好了卻不動作的狀態。
 */
export function OidcProviderCard({ settings }: { settings: Settings }) {
  return (
    <SettingsCard
      description="填入 issuer 即啟用，清空即關閉。其餘端點都由 issuer 的 discovery 文件取得。"
      title="OIDC"
    >
      <OidcProviderFields
        defaultValues={initialValues(settings)}
        secretIsSet={isSecretSet(settings, KEYS.clientSecret)}
      />
    </SettingsCard>
  );
}

function initialValues(settings: Settings): OidcSettingsValues {
  return {
    clientId: asString(readSetting(settings, KEYS.clientId), ""),
    // SEALED：讀不回值，永遠從空的開始。
    clientSecret: "",
    displayName: asString(readSetting(settings, KEYS.displayName), ""),
    groupClaim: asString(readSetting(settings, KEYS.groupClaim), "groups"),
    issuer: asString(readSetting(settings, KEYS.issuer), ""),
    linkByVerifiedEmail: asBoolean(
      readSetting(settings, KEYS.linkByVerifiedEmail),
      false
    ),
    requiredGroup: asString(readSetting(settings, KEYS.requiredGroup), ""),
    scopes: asScopes(
      readSetting(settings, KEYS.scopes),
      "openid profile email"
    ),
  };
}

function OidcProviderFields({
  defaultValues,
  secretIsSet,
}: {
  defaultValues: OidcSettingsValues;
  secretIsSet: boolean;
}) {
  const form = useSettingsForm<OidcSettingsValues>({
    defaultValues,
    encoders: ENCODERS,
    keys: KEYS,
    resolver: zodResolver(oidcSettingsSchema),
  });

  const issuer = useWatch({ control: form.control, name: "issuer" });
  const requiredGroup = useWatch({
    control: form.control,
    name: "requiredGroup",
  });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        {issuer.trim() !== "" && requiredGroup.trim() === "" && (
          <OpenToEveryoneWarning />
        )}

        <Controller
          control={form.control}
          name="issuer"
          render={({ field, fieldState }) => (
            <TextField
              description="必須是 https，且要與 provider 在 id_token 裡給的 iss claim 完全一字不差（規範要求）。清空代表關閉 OIDC 登入。"
              field={field}
              fieldState={fieldState}
              id="oidc-issuer"
              label="Issuer URL"
              placeholder="https://auth.example.com/application/o/tessera/"
            />
          )}
        />

        <Controller
          control={form.control}
          name="clientId"
          render={({ field, fieldState }) => (
            <TextField
              field={field}
              fieldState={fieldState}
              id="oidc-client-id"
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
              id="oidc-client-secret"
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
              description="以空白分隔。openid 缺了會自動補；email 是必要的，沒有 email 的登入會被拒絕。"
              field={field}
              fieldState={fieldState}
              id="oidc-scopes"
              label="Scopes"
              placeholder="openid profile email"
            />
          )}
        />

        <FieldSeparator />

        <Controller
          control={form.control}
          name="requiredGroup"
          render={({ field, fieldState }) => (
            <TextField
              description="只允許這個群組的人登入。留空代表 provider 認證過的任何帳號都能進來。"
              field={field}
              fieldState={fieldState}
              id="oidc-required-group"
              label="必要群組"
            />
          )}
        />

        <Controller
          control={form.control}
          name="groupClaim"
          render={({ field, fieldState }) => (
            <TextField
              description="群組放在哪個 claim。Authentik／Authelia 用 groups，Keycloak 用 realm_access.roles，部分 LDAP bridge 用 memberOf。點號可以選巢狀欄位。"
              field={field}
              fieldState={fieldState}
              id="oidc-group-claim"
              label="群組 claim"
              placeholder="groups"
            />
          )}
        />

        <FieldSeparator />

        <Controller
          control={form.control}
          name="displayName"
          render={({ field, fieldState }) => (
            <TextField
              description="登入按鈕上的文字，留空則用 issuer 的網域。"
              field={field}
              fieldState={fieldState}
              id="oidc-display-name"
              label="按鈕顯示名稱"
            />
          )}
        />

        <Controller
          control={form.control}
          name="linkByVerifiedEmail"
          render={({ field }) => (
            <SwitchField
              description="允許 provider 的身分綁到同 email 的既有帳號。開了也仍要 provider 說 email_verified。"
              field={field}
              id="oidc-link-by-email"
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

/**
 * `required_group` 留空是實質的安全問題，不只是「還沒填」。
 *
 * Provider 服務多個應用時，它認證的是「所有在它那裡有帳號的人」，不等於「該進這個
 * 站的人」—— 只設了 Google 又留空，結果是全世界任何 Google 帳號都能登入。
 */
function OpenToEveryoneWarning() {
  return (
    <div
      className="flex gap-3 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm"
      role="alert"
    >
      <IconAlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-500" />
      <div className="space-y-1">
        <p className="font-medium">目前這個 provider 的任何帳號都能登入</p>
        <p className="text-muted-foreground">
          「必要群組」留空代表不篩選。provider 認證的是所有在它那裡有帳號的人 ——
          如果那是 Google
          之類的公開服務，等於對全世界開放註冊。要限制的話，填一個群組名稱。
        </p>
      </div>
    </div>
  );
}
