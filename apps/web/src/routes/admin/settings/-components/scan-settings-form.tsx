import { zodResolver } from "@hookform/resolvers/zod";
import { IconAlertTriangle, IconInfoCircle } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { FieldGroup } from "@tessera/ui/components/field";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import { scanSettingsSchema } from "@/lib/admin/settings-schema";
import type { ScanSettingsValues } from "@/lib/admin/settings-schema";
import { asBoolean, asString, isSecretSet } from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { SealedField, SettingsNotice, TextField } from "./form-fields";
import { ScannerStatusCard } from "./scanner-status-card";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];

const KEYS = {
  authorization: "scan.authorization",
  url: "scan.url",
} satisfies SettingsKeyMap<ScanSettingsValues>;

const ENCODERS = {
  // SEALED: empty means unchanged, so the key is not sent.
  authorization: (value: string) => (value === "" ? undefined : value),
};

/**
 * Malware scanning. Tessera has no scanner of its own; the operator runs one
 * and points this at it. An empty `scan.url`, the default, means no scanning.
 * See `docs/scanning.md`.
 */
export function ScanSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <ScanSettingsFields
          authorizationIsSet={isSecretSet(settings, KEYS.authorization)}
          defaultValues={initialValues(settings)}
          encryptionEnabled={asBoolean(
            readSetting(settings, "features.encryption"),
            false
          )}
        />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): ScanSettingsValues {
  return {
    // SEALED: never read back, so it always starts empty.
    authorization: "",
    url: asString(readSetting(settings, KEYS.url), ""),
  };
}

function ScanSettingsFields({
  authorizationIsSet,
  defaultValues,
  encryptionEnabled,
}: {
  authorizationIsSet: boolean;
  defaultValues: ScanSettingsValues;
  encryptionEnabled: boolean;
}) {
  const form = useSettingsForm<ScanSettingsValues>({
    defaultValues,
    encoders: ENCODERS,
    keys: KEYS,
    resolver: zodResolver(scanSettingsSchema),
  });

  const url = useWatch({ control: form.control, name: "url" });
  const enabled = url.trim() !== "";

  return (
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      {!enabled && (
        <SettingsNotice
          icon={<IconInfoCircle />}
          title={m.settings_scan_form_off_title()}
          tone="info"
        >
          {m.settings_scan_form_off_body()}
        </SettingsNotice>
      )}

      {/* End-to-end encrypted shares are ciphertext and cannot be scanned, so with both on encryption is a way around scanning. */}
      {enabled && encryptionEnabled && (
        <SettingsNotice
          icon={<IconAlertTriangle />}
          title={m.settings_scan_form_encryption_title()}
          tone="warning"
        >
          {m.settings_scan_form_encryption_body()}
        </SettingsNotice>
      )}

      <SettingsCard
        description={m.settings_scan_service_description()}
        title={m.settings_scan_service_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="url"
            render={({ field, fieldState }) => (
              <TextField
                description={m.settings_scan_form_url_description()}
                field={field}
                fieldState={fieldState}
                id="scan-url"
                label={m.settings_scan_form_url_label()}
                placeholder="clamd://clamav:3310"
              />
            )}
          />

          <Controller
            control={form.control}
            name="authorization"
            render={({ field, fieldState }) => (
              <SealedField
                description={m.settings_scan_form_authorization_description()}
                field={field}
                fieldState={fieldState}
                id="scan-authorization"
                isSet={authorizationIsSet}
                label={m.settings_scan_form_authorization_label()}
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <ScannerStatusCard />

      <SettingsFormFooter form={form} />
    </form>
  );
}
