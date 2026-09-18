import {
  Field,
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
import type * as React from "react";
import type { ControllerRenderProps, FieldValues, Path } from "react-hook-form";

interface FieldStateLike {
  invalid: boolean;
  error?: { message?: string };
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
        placeholder={isSet ? "已設定 —— 留空代表不變更" : "尚未設定"}
        type="password"
        value={field.value ?? ""}
      />
      <FieldDescription>
        {isSet
          ? "目前已設定。這裡讀不回原值，留空就維持不變，填了才會覆寫。"
          : "目前未設定。填入後儲存即生效；之後這裡一樣不會顯示原值。"}
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
      <FieldLabel htmlFor={id}>
        {label}
        {description && <FieldDescription>{description}</FieldDescription>}
      </FieldLabel>
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
    <Field data-invalid={fieldState.invalid}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Select onValueChange={field.onChange} value={field.value}>
        <SelectTrigger id={id}>
          <SelectValue placeholder={placeholder} />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {options.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      {description && <FieldDescription>{description}</FieldDescription>}
      {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
    </Field>
  );
}
