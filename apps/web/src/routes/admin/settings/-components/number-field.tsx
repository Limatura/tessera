import { Input } from "@tessera/ui/components/input";
import { formatBytes } from "@tessera/ui/lib/format";
import type * as React from "react";
import type { ControllerRenderProps, FieldValues, Path } from "react-hook-form";

import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

import { SettingRow } from "./form-fields";

/** 數字後面那個單位，同時決定要不要附一個換算過的讀法。 */
export type NumberUnit =
  | "bytes"
  | "seconds"
  | "minutes"
  | "hours"
  | "days"
  | "times"
  | "kib";

const SECONDS_PER: Record<"second" | "minute" | "hour" | "day", number> = {
  day: 24 * 60 * 60,
  hour: 60 * 60,
  minute: 60,
  second: 1,
};

/**
 * 數字型 `Controller` 欄位，統一處理 `valueAsNumber` 轉換。
 *
 * 單位放在輸入框裡，不寫在標題的括號裡；時間跟位元組另外附一個人讀得懂的換算
 * （168 小時 ≈ 7 天、65536 bytes = 64 KB），不用自己心算。
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
 * 只有輸入框加單位加換算，不帶標題。給一列裡放兩格的地方用（限流的次數／時間）。
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
  const value = field.value as number | undefined;
  const hint = unit ? readable(value, unit) : undefined;

  return (
    <div className="flex flex-col gap-1">
      <div className="relative">
        <Input
          {...field}
          aria-invalid={invalid}
          aria-label={ariaLabel}
          className={unit ? "pr-16 tabular-nums" : "tabular-nums"}
          id={id}
          max={max}
          min={min}
          onChange={(event) => field.onChange(event.target.valueAsNumber)}
          type="number"
          value={Number.isNaN(value) ? "" : (value ?? "")}
        />
        {unit && (
          <span className="pointer-events-none absolute inset-y-0 right-3 flex items-center text-muted-foreground text-xs">
            {unitLabel(unit)}
          </span>
        )}
      </div>
      {hint && (
        <p className="text-right text-muted-foreground text-xs tabular-nums">
          {hint}
        </p>
      )}
    </div>
  );
}

/** 秒數換成最大的整數單位來讀：900 → 「15 分鐘」、3600 → 「1 小時」。 */
export function formatSeconds(seconds: number): string {
  const unit =
    (["day", "hour", "minute"] as const).find(
      (candidate) => seconds % SECONDS_PER[candidate] === 0
    ) ?? "second";
  return unitName(unit, seconds / SECONDS_PER[unit]);
}

/** 輸入框裡的單位字樣。用 `Intl` 的單位名稱，三種介面語言都不用另外翻。 */
function unitLabel(unit: NumberUnit): string {
  switch (unit) {
    case "bytes": {
      return "bytes";
    }
    case "kib": {
      return "KiB";
    }
    case "times": {
      return m.settings_number_unit_times();
    }
    default: {
      return unitName(unitSingular(unit), 2).replace(/^[\d\s.,]+/u, "");
    }
  }
}

function unitSingular(unit: "seconds" | "minutes" | "hours" | "days") {
  return unit.slice(0, -1) as "second" | "minute" | "hour" | "day";
}

/** `Intl.NumberFormat` 的單位寫法：zh-TW「7 天」、en「7 days」、ja「7 日」。 */
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

/**
 * 換一個比較好讀的單位。換不出更好讀的（本來就是最合適的單位、或是 0）就不顯示，
 * 免得每一格底下都掛一行重複的數字。
 */
function readable(value: number | undefined, unit: NumberUnit) {
  if (value === undefined || Number.isNaN(value) || value <= 0) {
    return;
  }

  if (unit === "bytes") {
    return value >= 1024 ? `= ${formatBytes(value)}` : undefined;
  }
  if (unit === "kib") {
    return value >= 1024 ? `= ${formatBytes(value * 1024)}` : undefined;
  }
  if (unit === "times") {
    return;
  }

  const seconds = value * SECONDS_PER[unitSingular(unit)];
  const target = (["day", "hour", "minute"] as const).find(
    (candidate) => seconds >= SECONDS_PER[candidate]
  );
  if (!target || target === unitSingular(unit)) {
    return;
  }

  const amount = seconds / SECONDS_PER[target];
  const sign = Number.isInteger(amount) ? "=" : "≈";
  return `${sign} ${unitName(target, amount)}`;
}
