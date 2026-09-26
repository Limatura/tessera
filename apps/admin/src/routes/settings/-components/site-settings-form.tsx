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

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import {
  SITE_DESCRIPTION_MAX,
  SITE_TITLE_MAX,
  siteSettingsSchema,
} from "@/lib/settings-schema";
import type { SiteSettingsValues } from "@/lib/settings-schema";
import { asString } from "@/lib/settings-values";
import * as m from "@/paraglide/messages";

import { TextField } from "./form-fields";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_TITLE = "Tessera";

const KEYS = {
  description: "site.description",
  title: "site.title",
} satisfies SettingsKeyMap<SiteSettingsValues>;

/**
 * 站台資訊：`site.title` / `site.description`。
 *
 * 跟 `appearance.*` 一樣在 `index.html` 送出前就寫進去，所以分頁標題和連結預覽
 * 不用等 script。分享頁的說明用分享自己的，不會被這裡蓋掉。
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
    <form onSubmit={form.submit}>
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
