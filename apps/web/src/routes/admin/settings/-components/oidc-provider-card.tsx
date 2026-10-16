import { zodResolver } from "@hookform/resolvers/zod";
import { IconAlertOctagon, IconAlertTriangle } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { FieldGroup, FieldSeparator } from "@tessera/ui/components/field";
import { useState } from "react";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import {
  LINK_BY_EMAIL_MODES,
  PROVIDER_REGISTRATION_MODES,
  oidcSettingsSchema,
} from "@/lib/admin/settings-schema";
import type { OidcSettingsValues } from "@/lib/admin/settings-schema";
import {
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
  SettingsNotice,
  TextField,
} from "./form-fields";
import { OidcDiscoveryField } from "./oidc-discovery-field";
import { ProviderCallbackUrls } from "./provider-callback-urls";
import {
  ProviderRegistrationNote,
  providerRegistrationOptions,
} from "./provider-registration-note";
import { SettingsFormFooter } from "./settings-form-footer";

type Settings = components["schemas"]["SettingsResponse"];

const KEYS = {
  allowRegistration: "auth.oidc.allow_registration",
  clientId: "auth.oidc.client_id",
  clientSecret: "auth.oidc.client_secret",
  displayName: "auth.oidc.display_name",
  groupClaim: "auth.oidc.group_claim",
  issuer: "auth.oidc.issuer",
  linkByEmail: "auth.oidc.link_by_email",
  requiredGroup: "auth.oidc.required_group",
  scopes: "auth.oidc.scopes",
} satisfies SettingsKeyMap<OidcSettingsValues>;

const ENCODERS = {
  // SEALED: empty means unchanged, so the key stays out of this batch.
  clientSecret: (value: string) => (value === "" ? undefined : value),
  scopes: (value: string) => toScopes(value),
};

/**
 * Any standard OIDC provider (Authentik, Authelia, Keycloak, Google, ...).
 *
 * No "enabled" switch: an empty `issuer` means off. A separate boolean would only add
 * a state where the issuer is filled in but the switch was forgotten, which looks
 * configured and does nothing.
 */
export function OidcProviderForm({ settings }: { settings: Settings }) {
  return (
    <OidcProviderFields
      defaultValues={initialValues(settings)}
      publicUrl={asString(readSetting(settings, "server.public_url"), "")}
      secretIsSet={isSecretSet(settings, KEYS.clientSecret)}
    />
  );
}

/** An empty issuer means off, see above. */
export function isOidcConfigured(settings: Settings): boolean {
  return asString(readSetting(settings, KEYS.issuer), "").trim() !== "";
}

function initialValues(settings: Settings): OidcSettingsValues {
  return {
    allowRegistration: asEnum(
      readSetting(settings, KEYS.allowRegistration),
      PROVIDER_REGISTRATION_MODES,
      "inherit"
    ),
    clientId: asString(readSetting(settings, KEYS.clientId), ""),
    // SEALED: cannot be read back, so it always starts empty.
    clientSecret: "",
    displayName: asString(readSetting(settings, KEYS.displayName), ""),
    groupClaim: asString(readSetting(settings, KEYS.groupClaim), "groups"),
    issuer: asString(readSetting(settings, KEYS.issuer), ""),
    linkByEmail: asEnum(
      readSetting(settings, KEYS.linkByEmail),
      LINK_BY_EMAIL_MODES,
      "never"
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
  const allowRegistration = useWatch({
    control: form.control,
    name: "allowRegistration",
  });
  const linkByEmail = useWatch({ control: form.control, name: "linkByEmail" });

  const configured = issuer.trim() !== "";
  const noGroup = requiredGroup.trim() === "";

  /*
   * The back-channel logout URL shows only after reading the discovery document, and
   * only when the provider declares support. Few do (Keycloak, Authelia, Auth0;
   * Authentik from 2025.10; Okta has its own scheme; Google and Synology none), so
   * showing it always would be noise for most. Without the document, no guessing.
   */
  const [backchannelLogout, setBackchannelLogout] = useState(false);

  /*
   * Likewise, only the document tells whether it issues `email_verified`. Unread
   * (null) means no hint: a provider without claims_supported may still send it.
   */
  const [claimsSupported, setClaimsSupported] = useState<string[] | null>(null);
  const verifiedClaimMissing =
    claimsSupported !== null && !claimsSupported.includes("email_verified");

  /*
   * shouldDirty: submit only picks touched fields, so without it a filled-in value
   * would stay on screen yet never be saved, worse than not filling it at all.
   * shouldValidate shows a wrong URL right away instead of on save.
   */
  const handleApplyDiscovery = (values: {
    issuer: string;
    scopes?: string;
    backchannelLogout: boolean;
    claimsSupported: string[] | null;
  }) => {
    setBackchannelLogout(values.backchannelLogout);
    setClaimsSupported(values.claimsSupported);
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
        {/*
          `always` with no group gets the strongest warning from ProviderRegistrationNote,
          not repeated here; with `never` only existing accounts get in, so "anyone can
          sign in" does not apply.
        */}
        {configured && noGroup && allowRegistration === "inherit" && (
          <OpenToEveryoneWarning />
        )}

        {configured && (
          <ProviderRegistrationNote
            mode={allowRegistration}
            openToWorld={
              noGroup
                ? {
                    body: m.settings_oidc_card_open_registration_body(),
                    title: m.settings_oidc_card_open_registration_title(),
                  }
                : undefined
            }
          />
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
          name="allowRegistration"
          render={({ field, fieldState }) => (
            <SelectField
              description={m.settings_provider_registration_field_description()}
              field={field}
              fieldState={fieldState}
              id="oidc-allow-registration"
              label={m.settings_provider_registration_field_label()}
              options={providerRegistrationOptions()}
            />
          )}
        />

        <Controller
          control={form.control}
          name="linkByEmail"
          render={({ field, fieldState }) => (
            <SelectField
              description={m.settings_oidc_card_link_by_email_description()}
              field={field}
              fieldState={fieldState}
              id="oidc-link-by-email"
              label={m.settings_oidc_card_link_by_email_label()}
              options={linkByEmailOptions()}
            />
          )}
        />

        {linkByEmail === "always" && (
          <SettingsNotice
            icon={<IconAlertOctagon />}
            title={m.settings_oidc_card_link_always_warning_title()}
            tone="danger"
          >
            {m.settings_oidc_card_link_always_warning_body()}
          </SettingsNotice>
        )}

        {linkByEmail === "when_verified" && verifiedClaimMissing && (
          <SettingsNotice
            icon={<IconAlertTriangle />}
            title={m.settings_oidc_card_link_unverified_warning_title()}
            tone="warning"
          >
            {m.settings_oidc_card_link_unverified_warning_body()}
          </SettingsNotice>
        )}

        <SettingsFormFooter form={form} surface="sheet" />
      </FieldGroup>
    </form>
  );
}

function linkByEmailOptions() {
  return [
    { label: m.settings_oidc_card_link_by_email_never(), value: "never" },
    {
      label: m.settings_oidc_card_link_by_email_when_verified(),
      value: "when_verified",
    },
    { label: m.settings_oidc_card_link_by_email_always(), value: "always" },
  ];
}

/**
 * An empty `required_group` is a real security problem, not just "not filled in yet": a
 * provider serving several apps authenticates everyone with an account there, not just
 * the people who should reach this site. With Google and no group, any Google account
 * in the world can sign in.
 */
function OpenToEveryoneWarning() {
  return (
    <SettingsNotice
      icon={<IconAlertTriangle />}
      title={m.settings_oidc_card_open_warning_title()}
      tone="warning"
    >
      {m.settings_oidc_card_open_warning_body()}
    </SettingsNotice>
  );
}
