import { zodResolver } from "@hookform/resolvers/zod";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { Switch } from "@tessera/ui/components/switch";
import { Textarea } from "@tessera/ui/components/textarea";
import { Controller, useForm } from "react-hook-form";

import { siteSettingsSchema } from "@/lib/settings-schema";
import type { SiteSettingsValues } from "@/lib/settings-schema";

const DEFAULT_VALUES: SiteSettingsValues = {
  description: "",
  seoKeywords: "",
  showAdminEntryOnHome: false,
  title: "Tessera",
};

export function SiteSettingsForm() {
  const { control, handleSubmit } = useForm<SiteSettingsValues>({
    defaultValues: DEFAULT_VALUES,
    resolver: zodResolver(siteSettingsSchema),
  });

  const onSubmit = handleSubmit((_values) => {
    // 待補：串接後端 API（server 尚無網頁基本資訊設定端點）
  });

  return (
    <form onSubmit={onSubmit}>
      <FieldGroup>
        <Controller
          control={control}
          name="title"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="site-title">網頁標題名稱</FieldLabel>
              <Input
                {...field}
                aria-invalid={fieldState.invalid}
                id="site-title"
                placeholder="Tessera"
              />
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />
        <Controller
          control={control}
          name="description"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="site-description">網頁敘述</FieldLabel>
              <Textarea
                {...field}
                aria-invalid={fieldState.invalid}
                id="site-description"
                rows={3}
              />
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />
        <Controller
          control={control}
          name="seoKeywords"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="site-seo-keywords">SEO 關鍵詞</FieldLabel>
              <Input
                {...field}
                aria-invalid={fieldState.invalid}
                id="site-seo-keywords"
                placeholder="檔案分享, 匿名上傳"
              />
              <FieldDescription>以逗號分隔多個關鍵詞。</FieldDescription>
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />
        <Controller
          control={control}
          name="showAdminEntryOnHome"
          render={({ field }) => (
            <Field orientation="horizontal">
              <FieldLabel htmlFor="site-show-admin-entry">
                在主頁顯示管理入口
                <FieldDescription>
                  在訪客可見的首頁加上一個連到後台的連結。
                </FieldDescription>
              </FieldLabel>
              <Switch
                checked={field.value}
                id="site-show-admin-entry"
                onCheckedChange={field.onChange}
              />
            </Field>
          )}
        />
        <Field orientation="horizontal">
          <Button type="submit">儲存</Button>
        </Field>
      </FieldGroup>
    </form>
  );
}
