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
              description={`分頁標題，也是連結預覽裡的站名。不可空白，最多 ${SITE_TITLE_MAX} 字。`}
              field={field}
              fieldState={fieldState}
              id="site-title"
              label="站名"
              placeholder={DEFAULT_TITLE}
            />
          )}
        />

        <Controller
          control={form.control}
          name="description"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="site-description">說明</FieldLabel>
              <Textarea
                {...field}
                aria-invalid={fieldState.invalid}
                id="site-description"
                rows={3}
              />
              <FieldDescription>
                有人貼出這個站本身的連結時，預覽卡片顯示的說明。留空就不輸出這個
                meta。最多 {SITE_DESCRIPTION_MAX} 字。
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
