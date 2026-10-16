import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup, FieldSeparator } from "@tessera/ui/components/field";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import {
  oauthSettingsSchema,
  PROVIDER_REGISTRATION_MODES,
} from "@/lib/admin/settings-schema";
import type { OauthSettingsValues } from "@/lib/admin/settings-schema";
import {
  asBoolean,
  asEnum,
  asScopes,
  asString,
  isSecretSet,
  toScopes,
} from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import {
  SealedField,
  SelectField,
  SwitchField,
  TextField,
} from "./form-fields";
import { ProviderCallbackUrls } from "./provider-callback-urls";
import {
  ProviderRegistrationNote,
  providerRegistrationOptions,
} from "./provider-registration-note";
import { SettingsFormFooter } from "./settings-form-footer";

type Settings = components["schemas"]["SettingsResponse"];

const ENCODERS = {
  // SEALED: empty means unchanged.
  clientSecret: (value: string) => (value === "" ? undefined : value),
  scopes: (value: string) => toScopes(value),
};

/**
 * GitHub or Discord.
 *
 * Neither is an OIDC provider (no id_token, no published discovery document), so the
 * backend has adapters with hardcoded endpoints. For the operator the settings match
 * the OIDC card, so this card looks the same.
 */
export function OauthProviderForm({
  settings,
  provider,
  title,
  scopesHint,
  baseUrlHint,
}: {
  settings: Settings;
  /** Prefix of the dotted keys, `auth.github` or `auth.discord`. */
  provider: "auth.github" | "auth.discord";
  title: string;
  scopesHint: string;
  /** Only GitHub uses base_url; when not given, the field is not drawn and the key is not sent. */
  baseUrlHint?: string;
}) {
  const keys = keysFor(provider, baseUrlHint !== undefined);

  return (
    <OauthProviderFields
      baseUrlHint={baseUrlHint}
      defaultValues={initialValues(settings, provider)}
      idPrefix={provider === "auth.github" ? "github" : "discord"}
      keys={keys}
      providerName={title}
      publicUrl={asString(readSetting(settings, "server.public_url"), "")}
      scopesHint={scopesHint}
      secretIsSet={isSecretSet(settings, `${provider}.client_secret`)}
    />
  );
}

/** A client ID is what turns a GitHub / Discord sign-in on. */
export function isOauthConfigured(
  settings: Settings,
  provider: "auth.github" | "auth.discord"
): boolean {
  return (
    asString(readSetting(settings, `${provider}.client_id`), "").trim() !== ""
  );
}

function keysFor(
  provider: string,
  withBaseUrl: boolean
): SettingsKeyMap<OauthSettingsValues> {
  return {
    allowRegistration: `${provider}.allow_registration`,
    // Discord ignores base_url, so its card neither draws nor sends it: a value the
    // provider never reads would suggest to the next reader that it does something.
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
    allowRegistration: asEnum(
      readSetting(settings, `${provider}.allow_registration`),
      PROVIDER_REGISTRATION_MODES,
      "inherit"
    ),
    baseUrl: asString(readSetting(settings, `${provider}.base_url`), ""),
    clientId: asString(readSetting(settings, `${provider}.client_id`), ""),
    // SEALED: cannot be read back, so it always starts empty.
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
  publicUrl,
  providerName,
}: {
  defaultValues: OauthSettingsValues;
  keys: SettingsKeyMap<OauthSettingsValues>;
  idPrefix: string;
  providerName: string;
  scopesHint: string;
  baseUrlHint?: string;
  secretIsSet: boolean;
  publicUrl: string;
}) {
  const form = useSettingsForm<OauthSettingsValues>({
    defaultValues,
    encoders: ENCODERS,
    keys,
    resolver: zodResolver(oauthSettingsSchema),
  });

  const allowRegistration = useWatch({
    control: form.control,
    name: "allowRegistration",
  });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        {/*
          GitHub and Discord have no groups to filter on: every account the provider
          authenticates gets in, so `always` always gets the strongest warning.
        */}
        <ProviderRegistrationNote
          mode={allowRegistration}
          openToWorld={{
            body: m.settings_provider_registration_world_body({
              provider: providerName,
            }),
            title: m.settings_provider_registration_world_title({
              provider: providerName,
            }),
          }}
        />

        {/* idPrefix is the path segment: `github` / `discord`. */}
        <ProviderCallbackUrls provider={idPrefix} publicUrl={publicUrl} />

        <Controller
          control={form.control}
          name="clientId"
          render={({ field, fieldState }) => (
            <TextField
              description={m.settings_oauth_card_client_id_description()}
              field={field}
              fieldState={fieldState}
              id={`${idPrefix}-client-id`}
              label={m.settings_oauth_card_client_id_label()}
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
              label={m.settings_oauth_card_client_secret_label()}
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
              label={m.settings_oauth_card_scopes_label()}
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
                label={m.settings_oauth_card_base_url_label()}
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
              description={m.settings_oauth_card_display_name_description()}
              field={field}
              fieldState={fieldState}
              id={`${idPrefix}-display-name`}
              label={m.settings_oauth_card_display_name_label()}
            />
          )}
        />

        <Controller
          control={form.control}
          name="allowRegistration"
          render={({ field, fieldState }) => (
            <SelectField
              description={m.settings_provider_registration_field_description()}
              field={field}
              fieldState={fieldState}
              id={`${idPrefix}-allow-registration`}
              label={m.settings_provider_registration_field_label()}
              options={providerRegistrationOptions()}
            />
          )}
        />

        <Controller
          control={form.control}
          name="linkByVerifiedEmail"
          render={({ field }) => (
            <SwitchField
              description={m.settings_oauth_card_link_by_email_description()}
              field={field}
              id={`${idPrefix}-link-by-email`}
              label={m.settings_oauth_card_link_by_email_label()}
            />
          )}
        />

        <SettingsFormFooter form={form} surface="sheet" />
      </FieldGroup>
    </form>
  );
}
