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
 * One setting row: title and description on the left, a narrow control on the right
 * (number, switch, select).
 *
 * Side by side only inside `FieldGroup`, a container query that puts them in a row when
 * the card is wide enough and stacks them on phones. Fields that need width, such as
 * URLs or multi-line text, use `TextField` instead.
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
  /** Width of the control column, `w-48` by default. */
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

/** A text `Controller` field. Pairs with `NumberField`, minus the number conversion. */
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
 * A SEALED field (`*.client_secret`, `mail.username`, `mail.password`).
 *
 * `GET /admin/settings` only says whether it is set, never the value, so the input
 * always starts empty: empty means unchanged, filled means overwrite. The usual "load
 * the value and edit" would wipe an existing secret to an empty string on save.
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
  /** Whether the server has a value for this key. */
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

/** An option's value is the string the server accepts. */
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
          {/* The trigger shows the raw value by default; show the translated label instead. */}
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
 * A notice inside a form. `warning` means "this setting will cause trouble", `danger`
 * means "saving this locks you out", `info` is extra explanation. One component so the
 * forms stop hand-rolling their own with mismatched colors and spacing.
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
