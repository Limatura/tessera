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
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { Controller, useForm } from "react-hook-form";

import { themeSettingsSchema } from "@/lib/settings-schema";
import type { ThemeSettingsValues } from "@/lib/settings-schema";

const DEFAULT_VALUES: ThemeSettingsValues = {
  accentColor: "#6366f1",
  mode: "system",
};

const MODE_OPTIONS: { value: ThemeSettingsValues["mode"]; label: string }[] = [
  { label: "跟隨系統", value: "system" },
  { label: "淺色", value: "light" },
  { label: "深色", value: "dark" },
];

export function ThemeSettingsForm() {
  const { control, handleSubmit } = useForm<ThemeSettingsValues>({
    defaultValues: DEFAULT_VALUES,
    resolver: zodResolver(themeSettingsSchema),
  });

  const onSubmit = handleSubmit((_values) => {
    // 待補：串接後端 API（server 尚無介面主題設定端點）
  });

  return (
    <form onSubmit={onSubmit}>
      <FieldGroup>
        <Controller
          control={control}
          name="mode"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="theme-mode">預設主題</FieldLabel>
              <Select onValueChange={field.onChange} value={field.value}>
                <SelectTrigger id="theme-mode">
                  <SelectValue placeholder="選擇主題" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {MODE_OPTIONS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        {option.label}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
              <FieldDescription>
                訪客首次造訪時套用的預設主題，之後仍可自行切換。
              </FieldDescription>
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />
        <Controller
          control={control}
          name="accentColor"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="theme-accent-color">主色調</FieldLabel>
              <div className="flex items-center gap-2">
                <Input
                  className="h-9 w-14 p-1"
                  id="theme-accent-color"
                  onChange={field.onChange}
                  type="color"
                  value={field.value}
                />
                <Input
                  aria-label="主色調色碼"
                  className="max-w-32"
                  onChange={field.onChange}
                  value={field.value}
                />
              </div>
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
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
