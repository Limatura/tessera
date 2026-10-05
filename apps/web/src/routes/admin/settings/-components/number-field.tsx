import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import type * as React from "react";
import type { ControllerRenderProps, FieldValues, Path } from "react-hook-form";

/** 數字型 `Controller` 欄位，統一處理 `valueAsNumber` 轉換。 */
export function NumberField<TValues extends FieldValues>({
  field,
  fieldState,
  id,
  label,
  description,
  min,
  max,
}: {
  field: ControllerRenderProps<TValues, Path<TValues>>;
  fieldState: { invalid: boolean; error?: { message?: string } };
  id: string;
  label: string;
  description?: React.ReactNode;
  min?: number;
  max?: number;
}) {
  return (
    <Field data-invalid={fieldState.invalid}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Input
        {...field}
        aria-invalid={fieldState.invalid}
        id={id}
        max={max}
        min={min}
        onChange={(event) => field.onChange(event.target.valueAsNumber)}
        type="number"
        value={field.value ?? ""}
      />
      {description && <FieldDescription>{description}</FieldDescription>}
      {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
    </Field>
  );
}
