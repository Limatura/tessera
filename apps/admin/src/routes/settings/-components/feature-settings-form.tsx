import { zodResolver } from "@hookform/resolvers/zod";
import { IconAlertTriangle } from "@tabler/icons-react";
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
import { featureSettingsSchema } from "@/lib/settings-schema";
import type { FeatureSettingsValues } from "@/lib/settings-schema";
import { asBoolean, asString } from "@/lib/settings-values";
import * as m from "@/paraglide/messages";

import { SwitchField } from "./form-fields";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const KEYS = {
  accounts: "features.accounts",
  admin: "features.admin",
  ogLayout: "og.layout",
} satisfies SettingsKeyMap<FeatureSettingsValues>;

export function FeatureSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <FeatureSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): FeatureSettingsValues {
  return {
    accounts: asBoolean(readSetting(settings, KEYS.accounts), true),
    admin: asBoolean(readSetting(settings, KEYS.admin), true),
    ogLayout: asString(readSetting(settings, KEYS.ogLayout), ""),
  };
}

function FeatureSettingsFields({
  defaultValues,
}: {
  defaultValues: FeatureSettingsValues;
}) {
  const form = useSettingsForm<FeatureSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(featureSettingsSchema),
  });

  const admin = useWatch({ control: form.control, name: "admin" });
  const accounts = useWatch({ control: form.control, name: "accounts" });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <p className="text-muted-foreground text-sm">
          {m.settings_feature_form_intro()}
        </p>

        {!admin && <AdminLockoutWarning />}

        <Controller
          control={form.control}
          name="admin"
          render={({ field }) => (
            <SwitchField
              description={m.settings_feature_form_admin_description()}
              field={field}
              id="features-admin"
              label={m.settings_feature_form_admin_label()}
            />
          )}
        />

        <Controller
          control={form.control}
          name="accounts"
          render={({ field }) => (
            <SwitchField
              description={m.settings_feature_form_accounts_description()}
              field={field}
              id="features-accounts"
              label={m.settings_feature_form_accounts_label()}
            />
          )}
        />

        {!accounts && admin && (
          <p className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
            {m.settings_feature_form_accounts_off_warning()}
          </p>
        )}

        <FieldSeparator />

        <Controller
          control={form.control}
          name="ogLayout"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="features-og-layout">
                {m.settings_feature_form_og_layout_label()}
              </FieldLabel>
              <Textarea
                {...field}
                aria-invalid={fieldState.invalid}
                className="font-mono text-xs"
                id="features-og-layout"
                rows={6}
              />
              <FieldDescription>
                {m.settings_feature_form_og_layout_description_1()}{" "}
                <code>div</code>、<code>span</code>、<code>style</code>
                {m.settings_feature_form_og_layout_description_2()}{" "}
                <code>{"{{size}}"}</code>
                {m.settings_feature_form_og_layout_description_3()}
              </FieldDescription>
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
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
 * 關掉 `features.admin` 會把 operator 鎖在門外,而且面板自己救不回來。
 *
 * 後端允許這個寫入(它不在 FIXED_AT_BOOT),所以攔不住的只有「知不知道自己在做什麼」。
 */
function AdminLockoutWarning() {
  return (
    <div
      className="flex gap-3 rounded-md border-2 border-destructive/50 bg-destructive/10 p-3 text-sm"
      role="alert"
    >
      <IconAlertTriangle className="mt-0.5 size-4 shrink-0 text-destructive" />
      <div className="space-y-1">
        <p className="font-medium">
          {m.settings_feature_form_admin_lockout_title()}
        </p>
        <p className="text-muted-foreground">
          {m.settings_feature_form_admin_lockout_description_1()}{" "}
          <code>TESSERA_FEATURES__ADMIN=true</code>{" "}
          {m.settings_feature_form_admin_lockout_description_2()}
        </p>
      </div>
    </div>
  );
}
