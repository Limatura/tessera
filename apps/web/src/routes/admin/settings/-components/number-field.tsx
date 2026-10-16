import { Input } from "@tessera/ui/components/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import * as React from "react";
import type { ControllerRenderProps, FieldValues, Path } from "react-hook-form";

import type { DisplayUnit, StoredUnit } from "@/lib/admin/units";
import {
  fromDisplay,
  isSizeUnit,
  pickUnit,
  toDisplay,
  unitsFor,
} from "@/lib/admin/units";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

import { SettingRow } from "./form-fields";

/** The unit after the number, which also decides whether a converted reading is added. */
export type NumberUnit = StoredUnit | "times";

const LEADING_NUMBER = /^[\d\s.,]+/u;

const SECONDS_PER: Record<"second" | "minute" | "hour" | "day", number> = {
  day: 24 * 60 * 60,
  hour: 60 * 60,
  minute: 60,
  second: 1,
};

/**
 * A number `Controller` field with the `valueAsNumber` conversion in one place.
 *
 * The unit sits beside the number, not in parentheses in the title. Times and sizes
 * get a unit menu, so no one has to do the arithmetic.
 */
export function NumberField<TValues extends FieldValues>({
  field,
  fieldState,
  id,
  label,
  description,
  min,
  max,
  unit,
}: {
  field: ControllerRenderProps<TValues, Path<TValues>>;
  fieldState: { invalid: boolean; error?: { message?: string } };
  id: string;
  label: string;
  description?: React.ReactNode;
  min?: number;
  max?: number;
  unit?: NumberUnit;
}) {
  return (
    <SettingRow
      description={description}
      error={fieldState.invalid ? fieldState.error : undefined}
      id={id}
      label={label}
    >
      <NumberInput
        field={field}
        id={id}
        invalid={fieldState.invalid}
        max={max}
        min={min}
        unit={unit}
      />
    </SettingRow>
  );
}

/**
 * Only the input with its unit, no title. For rows holding two inputs (the rate
 * limit's count and window).
 *
 * Times and sizes get a unit menu beside the number instead of a fixed unit:
 * "15 minutes" and "2 GB" are typed as such, not as 900 and 2147483648. The form
 * value stays in the unit the server stores; only the display changes. The
 * menu starts on the largest unit the value is a whole number of, and switching
 * it keeps the amount (15 minutes → 900 seconds), not the number.
 */
export function NumberInput<TValues extends FieldValues>({
  field,
  id,
  invalid,
  min,
  max,
  unit,
  ariaLabel,
}: {
  field: ControllerRenderProps<TValues, Path<TValues>>;
  id: string;
  invalid: boolean;
  min?: number;
  max?: number;
  unit?: NumberUnit;
  ariaLabel?: string;
}) {
  if (unit && unit !== "times") {
    return (
      <UnitNumberInput
        ariaLabel={ariaLabel}
        field={field}
        id={id}
        invalid={invalid}
        max={max}
        min={min}
        stored={unit}
      />
    );
  }

  const value = field.value as number | undefined;
  return (
    <div className="relative">
      <Input
        {...field}
        aria-invalid={invalid}
        aria-label={ariaLabel}
        className={unit ? "pr-12 tabular-nums" : "tabular-nums"}
        id={id}
        max={max}
        min={min}
        onChange={(event) => field.onChange(event.target.valueAsNumber)}
        type="number"
        value={value === undefined || Number.isNaN(value) ? "" : value}
      />
      {unit && (
        <span className="pointer-events-none absolute inset-y-0 right-3 flex items-center text-muted-foreground text-sm">
          {m.settings_number_unit_times()}
        </span>
      )}
    </div>
  );
}

function UnitNumberInput<TValues extends FieldValues>({
  field,
  id,
  invalid,
  min,
  max,
  stored,
  ariaLabel,
}: {
  field: ControllerRenderProps<TValues, Path<TValues>>;
  id: string;
  invalid: boolean;
  min?: number;
  max?: number;
  stored: StoredUnit;
  ariaLabel?: string;
}) {
  const value = field.value as number | undefined;
  const [unit, setUnit] = React.useState<DisplayUnit>(() =>
    pickUnit(value, stored)
  );
  // Mid-typing text ("0.", "1.50") must not be rewritten from the rounded
  // stored value, so keep the draft until blur.
  const [draft, setDraft] = React.useState<string | null>(null);
  // A value that changes from outside (Discard, a reset) re-picks the unit, so
  // 900 seconds does not come back as 0.25 hours. While typing there is a
  // draft, and the admin's own unit is kept.
  const [previous, setPrevious] = React.useState(value);
  if (!Object.is(value, previous)) {
    setPrevious(value);
    if (draft === null) {
      setUnit(pickUnit(value, stored));
    }
  }

  const shown =
    value === undefined || Number.isNaN(value)
      ? ""
      : toDisplay(value, stored, unit);

  const handleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setDraft(event.target.value);
    const amount = event.target.valueAsNumber;
    field.onChange(
      Number.isNaN(amount) ? amount : fromDisplay(amount, stored, unit)
    );
  };

  return (
    <div className="flex gap-2">
      <Input
        {...field}
        aria-invalid={invalid}
        aria-label={ariaLabel}
        className="min-w-0 flex-1 tabular-nums"
        id={id}
        max={max === undefined ? undefined : toDisplay(max, stored, unit)}
        min={min === undefined ? undefined : toDisplay(min, stored, unit)}
        onBlur={() => {
          setDraft(null);
          field.onBlur();
        }}
        onChange={handleChange}
        step="any"
        type="number"
        value={draft ?? shown}
      />
      <Select
        onValueChange={(next) => {
          setDraft(null);
          setUnit(next as DisplayUnit);
        }}
        value={unit}
      >
        <SelectTrigger
          aria-label={m.settings_number_unit_aria()}
          className="w-24 shrink-0"
        >
          <SelectValue>{(next: DisplayUnit) => unitLabel(next)}</SelectValue>
        </SelectTrigger>
        <SelectContent alignItemWithTrigger={false}>
          {unitsFor(stored).map((option) => (
            <SelectItem key={option} value={option}>
              {unitLabel(option)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

/** Seconds in the largest whole unit: 900 → "15 minutes", 3600 → "1 hour". */
export function formatSeconds(seconds: number): string {
  const unit =
    (["day", "hour", "minute"] as const).find(
      (candidate) => seconds % SECONDS_PER[candidate] === 0
    ) ?? "second";
  return unitName(unit, seconds / SECONDS_PER[unit]);
}

/**
 * A unit's name in the menu. Time units use `Intl` unit names ("minutes",
 * "分鐘"), so the UI languages need no translations of their own.
 */
function unitLabel(unit: DisplayUnit): string {
  if (isSizeUnit(unit)) {
    return unit;
  }
  return unitName(unit, 2).replace(LEADING_NUMBER, "");
}

/**
 * How `Intl.NumberFormat` writes a unit, e.g. "7 days" in en, localized
 * likewise in zh-TW and ja.
 */
function unitName(
  unit: "second" | "minute" | "hour" | "day",
  amount: number
): string {
  return new Intl.NumberFormat(getLocale(), {
    maximumFractionDigits: 1,
    style: "unit",
    unit,
    unitDisplay: "long",
  }).format(amount);
}
