import { zodResolver } from "@hookform/resolvers/zod";
import { IconAlertTriangle } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { FieldGroup, FieldSeparator } from "@tessera/ui/components/field";
import { useState } from "react";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import { oidcSettingsSchema } from "@/lib/admin/settings-schema";
import type { OidcSettingsValues } from "@/lib/admin/settings-schema";
import {
  asBoolean,
  asScopes,
  asString,
  isSecretSet,
  toScopes,
} from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { SealedField, SwitchField, TextField } from "./form-fields";
import { OidcDiscoveryField } from "./oidc-discovery-field";
import { ProviderCallbackUrls } from "./provider-callback-urls";
import { ProviderRegistrationNote } from "./provider-registration-note";
import { SettingsCard } from "./settings-card";
import { SettingsFormFooter } from "./settings-form-footer";

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
    <SettingsCard description={m.settings_oidc_card_description()} title="OIDC">
      <OidcProviderFields
        defaultValues={initialValues(settings)}
        publicUrl={asString(readSetting(settings, "server.public_url"), "")}
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
  publicUrl,
}: {
  defaultValues: OidcSettingsValues;
  secretIsSet: boolean;
  publicUrl: string;
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
  const scopes = useWatch({ control: form.control, name: "scopes" });

  /*
   * 只有讀過 discovery 文件、而且對方宣告支援時才顯示 back-channel logout 的位址。
   *
   * 支援它的 provider 是少數（Keycloak、Authelia、Auth0，Authentik 要 2025.10
   * 以後；Okta 自己另做一套，Google 跟 Synology 沒有），所以無條件顯示等於給
   * 大多數人一個永遠用不到、又得自己判斷要不要填的欄位。沒讀過文件就當不知道，
   * 不猜。
   */
  const [backchannelLogout, setBackchannelLogout] = useState(false);

  /*
   * shouldDirty：送出時只挑動過的欄位，沒標記的話帶入的值會留在畫面上卻存不下去
   * —— 那比沒帶入更糟。shouldValidate 讓貼錯的網址當場顯示，不用等按儲存。
   */
  const handleApplyDiscovery = (values: {
    issuer: string;
    scopes?: string;
    backchannelLogout: boolean;
  }) => {
    setBackchannelLogout(values.backchannelLogout);
    form.setValue("issuer", values.issuer, {
      shouldDirty: true,
      shouldValidate: true,
    });
    if (values.scopes !== undefined) {
      form.setValue("scopes", values.scopes, {
        shouldDirty: true,
        shouldValidate: true,
      });
    }
  };

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        {issuer.trim() !== "" && requiredGroup.trim() === "" && (
          <OpenToEveryoneWarning />
        )}

        <OidcDiscoveryField
          issuer={issuer}
          onApply={handleApplyDiscovery}
          scopes={scopes}
        />

        <ProviderCallbackUrls
          provider="oidc"
          publicUrl={publicUrl}
          showBackchannel={backchannelLogout}
        />

        <ProviderRegistrationNote />

        <Controller
          control={form.control}
          name="issuer"
          render={({ field, fieldState }) => (
            <TextField
              description={m.settings_oidc_card_issuer_description()}
              field={field}
              fieldState={fieldState}
              id="oidc-issuer"
              label={m.settings_oidc_card_issuer_label()}
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
              label={m.settings_oidc_card_client_id_label()}
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
              label={m.settings_oidc_card_client_secret_label()}
            />
          )}
        />

        <Controller
          control={form.control}
          name="scopes"
          render={({ field, fieldState }) => (
            <TextField
              description={m.settings_oidc_card_scopes_description()}
              field={field}
              fieldState={fieldState}
              id="oidc-scopes"
              label={m.settings_oidc_card_scopes_label()}
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
              description={m.settings_oidc_card_required_group_description()}
              field={field}
              fieldState={fieldState}
              id="oidc-required-group"
              label={m.settings_oidc_card_required_group_label()}
            />
          )}
        />

        <Controller
          control={form.control}
          name="groupClaim"
          render={({ field, fieldState }) => (
            <TextField
              description={m.settings_oidc_card_group_claim_description()}
              field={field}
              fieldState={fieldState}
              id="oidc-group-claim"
              label={m.settings_oidc_card_group_claim_label()}
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
              description={m.settings_oidc_card_display_name_description()}
              field={field}
              fieldState={fieldState}
              id="oidc-display-name"
              label={m.settings_oidc_card_display_name_label()}
            />
          )}
        />

        <Controller
          control={form.control}
          name="linkByVerifiedEmail"
          render={({ field }) => (
            <SwitchField
              description={m.settings_oidc_card_link_by_email_description()}
              field={field}
              id="oidc-link-by-email"
              label={m.settings_oidc_card_link_by_email_label()}
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
        <p className="font-medium">
          {m.settings_oidc_card_open_warning_title()}
        </p>
        <p className="text-muted-foreground">
          {m.settings_oidc_card_open_warning_body()}
        </p>
      </div>
    </div>
  );
}
