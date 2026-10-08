import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
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
import { Switch } from "@tessera/ui/components/switch";
import { cn } from "@tessera/ui/lib/utils";
import type * as React from "react";
import type { ControllerRenderProps, FieldValues, Path } from "react-hook-form";

import * as m from "@/paraglide/messages";

interface FieldStateLike {
  invalid: boolean;
  error?: { message?: string };
}

/**
 * 一列設定：左邊標題跟說明，右邊一個窄的控制項（數字、開關、下拉）。
 *
 * 放在 `FieldGroup` 裡才會左右排 —— 它是 container query，卡片夠寬才並排，手機上
 * 自動疊成上下。網址、多行文字這種要寬度的欄位不用這個，用 `TextField`。
 */
export function SettingRow({
  id,
  label,
  description,
  error,
  controlClassName,
  children,
}: {
  id?: string;
  label: string;
  description?: React.ReactNode;
  error?: { message?: string };
  /** 控制項那一欄的寬度，預設 `w-48`。 */
  controlClassName?: string;
  children: React.ReactNode;
}) {
  return (
    <Field data-invalid={error !== undefined} orientation="responsive">
      <FieldContent>
        <FieldLabel htmlFor={id}>{label}</FieldLabel>
        {description && <FieldDescription>{description}</FieldDescription>}
        {error && <FieldError errors={[error]} />}
      </FieldContent>
      <div
        className={cn(
          "flex shrink-0 flex-col gap-1 @md/field-group:w-48",
          controlClassName
        )}
      >
        {children}
      </div>
    </Field>
  );
}

/** 文字型 `Controller` 欄位。跟 `NumberField` 一組，差別只在不做數字轉換。 */
export function TextField<TValues extends FieldValues>({
  field,
  fieldState,
  id,
  label,
  description,
  placeholder,
}: {
  field: ControllerRenderProps<TValues, Path<TValues>>;
  fieldState: FieldStateLike;
  id: string;
  label: string;
  description?: React.ReactNode;
  placeholder?: string;
}) {
  return (
    <Field data-invalid={fieldState.invalid}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Input
        {...field}
        aria-invalid={fieldState.invalid}
        id={id}
        placeholder={placeholder}
        value={field.value ?? ""}
      />
      {description && <FieldDescription>{description}</FieldDescription>}
      {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
    </Field>
  );
}

/**
 * SEALED 欄位（`*.client_secret`、`mail.username`、`mail.password`）。
 *
 * `GET /admin/settings` 只回「有沒有設過」，不回值，所以這一格永遠從空的開始：
 * **留空 = 不改動，有填 = 覆寫**。用常規的「載入現值再編輯」會讓使用者一按儲存就把
 * 既有的 secret 清成空字串。
 */
export function SealedField<TValues extends FieldValues>({
  field,
  fieldState,
  id,
  label,
  isSet,
  description,
}: {
  field: ControllerRenderProps<TValues, Path<TValues>>;
  fieldState: FieldStateLike;
  id: string;
  label: string;
  /** server 說這個 key 目前有沒有值。 */
  isSet: boolean;
  description?: React.ReactNode;
}) {
  return (
    <Field data-invalid={fieldState.invalid}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Input
        {...field}
        aria-invalid={fieldState.invalid}
        autoComplete="off"
        id={id}
        placeholder={
          isSet
            ? m.settings_form_fields_sealed_placeholder_set()
            : m.settings_form_fields_sealed_placeholder_unset()
        }
        type="password"
        value={field.value ?? ""}
      />
      <FieldDescription>
        {isSet
          ? m.settings_form_fields_sealed_description_set()
          : m.settings_form_fields_sealed_description_unset()}
        {description && <> {description}</>}
      </FieldDescription>
      {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
    </Field>
  );
}

/** 布林型 `Controller` 欄位。 */
export function SwitchField<TValues extends FieldValues>({
  field,
  id,
  label,
  description,
}: {
  field: ControllerRenderProps<TValues, Path<TValues>>;
  id: string;
  label: string;
  description?: React.ReactNode;
}) {
  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel htmlFor={id}>{label}</FieldLabel>
        {description && <FieldDescription>{description}</FieldDescription>}
      </FieldContent>
      <Switch
        checked={field.value === true}
        id={id}
        onCheckedChange={field.onChange}
      />
    </Field>
  );
}

/** 下拉選單型 `Controller` 欄位。選項的 value 直接就是 server 收的字串。 */
export function SelectField<TValues extends FieldValues>({
  field,
  fieldState,
  id,
  label,
  description,
  options,
  placeholder,
}: {
  field: ControllerRenderProps<TValues, Path<TValues>>;
  fieldState: FieldStateLike;
  id: string;
  label: string;
  description?: React.ReactNode;
  options: readonly { value: string; label: string }[];
  placeholder?: string;
}) {
  return (
    <SettingRow
      controlClassName="@md/field-group:w-60"
      description={description}
      error={fieldState.invalid ? fieldState.error : undefined}
      id={id}
      label={label}
    >
      <Select onValueChange={field.onChange} value={field.value}>
        <SelectTrigger className="w-full" id={id}>
          {/* 觸發按鈕預設顯示 raw value，要換回翻譯過的 label。 */}
          <SelectValue placeholder={placeholder}>
            {(value: string) =>
              options.find((option) => option.value === value)?.label ?? value
            }
          </SelectValue>
        </SelectTrigger>
        <SelectContent alignItemWithTrigger={false}>
          <SelectGroup>
            {options.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </SettingRow>
  );
}

/**
 * 表單裡的提醒框。`warning` 是「這樣設會出事」，`danger` 是「存下去會把自己鎖在外面」，
 * `info` 只是補充說明。各表單原本各自手刻一份，顏色跟間距不一致。
 */
export function SettingsNotice({
  tone = "info",
  title,
  icon,
  children,
}: {
  tone?: "info" | "warning" | "danger";
  title?: string;
  icon?: React.ReactNode;
  children?: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        "flex gap-3 rounded-lg border p-3 text-sm",
        tone === "info" && "border-border bg-muted/40",
        tone === "warning" && "border-amber-500/40 bg-amber-500/10",
        tone === "danger" && "border-destructive/50 bg-destructive/10"
      )}
      role={tone === "info" ? undefined : "alert"}
    >
      {icon && (
        <span
          className={cn(
            "mt-0.5 shrink-0 [&_svg]:size-4",
            tone === "warning" && "text-amber-600 dark:text-amber-500",
            tone === "danger" && "text-destructive",
            tone === "info" && "text-muted-foreground"
          )}
        >
          {icon}
        </span>
      )}
      <div className="flex min-w-0 flex-col gap-1">
        {title && <p className="font-medium">{title}</p>}
        {children && <div className="text-muted-foreground">{children}</div>}
      </div>
    </div>
  );
}
