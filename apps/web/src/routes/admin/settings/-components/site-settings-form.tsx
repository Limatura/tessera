import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Textarea } from "@tessera/ui/components/textarea";
import { Controller } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import {
  SITE_DESCRIPTION_MAX,
  SITE_LANGUAGES,
  SITE_TITLE_MAX,
  siteSettingsSchema,
} from "@/lib/admin/settings-schema";
import type { SiteSettingsValues } from "@/lib/admin/settings-schema";
import { asEnum, asString } from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { SelectField, TextField } from "./form-fields";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_TITLE = "Tessera";

const KEYS = {
  description: "site.description",
  language: "site.language",
  title: "site.title",
} satisfies SettingsKeyMap<SiteSettingsValues>;

/**
 * Each language is named in that language: someone looking for Japanese
 * recognises "日本語", not necessarily "Japanese".
 */
const LANGUAGE_OPTIONS = [
  { label: "English", value: "en" },
  { label: "繁體中文", value: "zh-TW" },
  { label: "日本語", value: "ja" },
] as const;

/**
 * Site information: `site.title`, `site.description`, `site.language`.
 *
 * Like `appearance.*`, written into `index.html` before it is sent, so the tab
 * title and link previews do not wait for scripts. A share page uses its own
 * description and is not overridden by this one.
 */
export function SiteSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <SiteSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): SiteSettingsValues {
  return {
    description: asString(readSetting(settings, KEYS.description), ""),
    language: asEnum(
      readSetting(settings, KEYS.language),
      SITE_LANGUAGES,
      "en"
    ),
    title: asString(readSetting(settings, KEYS.title), DEFAULT_TITLE),
  };
}

function SiteSettingsFields({
  defaultValues,
}: {
  defaultValues: SiteSettingsValues;
}) {
  const form = useSettingsForm<SiteSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(siteSettingsSchema),
  });

  return (
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      <SettingsCard
        description={m.settings_site_identity_description()}
        title={m.settings_site_identity_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="title"
            render={({ field, fieldState }) => (
              <TextField
                description={m.settings_site_form_title_description({
                  max: SITE_TITLE_MAX,
                })}
                field={field}
                fieldState={fieldState}
                id="site-title"
                label={m.settings_site_form_title_label()}
                placeholder={DEFAULT_TITLE}
              />
            )}
          />

          <Controller
            control={form.control}
            name="description"
            render={({ field, fieldState }) => (
              <Field data-invalid={fieldState.invalid}>
                <FieldLabel htmlFor="site-description">
                  {m.settings_site_form_description_label()}
                </FieldLabel>
                <Textarea
                  {...field}
                  aria-invalid={fieldState.invalid}
                  id="site-description"
                  rows={3}
                />
                <FieldDescription>
                  {m.settings_site_form_description_description({
                    max: SITE_DESCRIPTION_MAX,
                  })}
                </FieldDescription>
                {fieldState.invalid && (
                  <FieldError errors={[fieldState.error]} />
                )}
              </Field>
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsCard
        description={m.settings_site_preview_description()}
        title={m.settings_site_preview_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="language"
            render={({ field, fieldState }) => (
              <SelectField
                description={m.settings_site_form_language_description()}
                field={field}
                fieldState={fieldState}
                id="site-language"
                label={m.settings_site_form_language_label()}
                options={LANGUAGE_OPTIONS}
              />
            )}
          />
        </FieldGroup>
      </SettingsCard>

      <SettingsFormFooter form={form} />
    </form>
  );
}
