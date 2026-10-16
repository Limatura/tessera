import {
  Field,
  FieldDescription,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { useState } from "react";

import * as m from "@/paraglide/messages";

/** MiB per unit in the size menu beside a custom limit. */
const SIZE_UNITS = { GB: 1024, MB: 1 } as const;
type SizeUnit = keyof typeof SIZE_UNITS;

/**
 * A quota field has three states, and `null` and `0` mean different things.
 * `null` follows the level above (a role follows the site, an account follows
 * its role, and changes there carry through); `0` means no limit at this level.
 * A single number input cannot express that, so it is split into mode + value.
 */
const MIB = 1024 * 1024;

export type ByteLimitMode = "inherit" | "unlimited" | "custom";

/**
 * The `inherit` label differs per field and comes from the caller. Read at call
 * time so a language switch applies immediately.
 */
function modeLabels(): Record<ByteLimitMode, string> {
  return {
    custom: m.roles_byte_limit_custom(),
    inherit: m.roles_byte_limit_inherit(),
    unlimited: m.roles_byte_limit_unlimited(),
  };
}

export function modeOf(value: number | null | undefined): ByteLimitMode {
  if (value === null || value === undefined) {
    return "inherit";
  }
  return value === 0 ? "unlimited" : "custom";
}

/** `null` clears the value; `0` means unlimited. */
export function toBytes(mode: ByteLimitMode, mib: number): number | null {
  if (mode === "inherit") {
    return null;
  }
  return mode === "unlimited" ? 0 : Math.round(mib * MIB);
}

export function toMib(value: number | null | undefined): number {
  return value && value > 0 ? Math.round(value / MIB) : 0;
}

export function ByteLimitField({
  id,
  label,
  inheritLabel,
  mode,
  mib,
  onModeChange,
  onMibChange,
  disabled,
  inheritDescription,
  unlimitedDescription,
  customDescription,
}: {
  id: string;
  label: string;
  /** Omit when there is no level above to follow: the menu then has no inherit option. */
  inheritLabel?: string;
  /**
   * Description for the inherit mode. The default talks about the site; an
   * account inherits from its role instead.
   */
  inheritDescription?: string;
  /** Description for the unlimited mode. The default talks about a role. */
  unlimitedDescription?: string;
  /** Description for the custom mode. */
  customDescription?: string;
  mode: ByteLimitMode;
  mib: number;
  onModeChange: (mode: ByteLimitMode) => void;
  onMibChange: (mib: number) => void;
  disabled?: boolean;
}) {
  const labels = modeLabels();
  // Starts on GB when the limit is a whole number of them, like the settings inputs.
  const [unit, setUnit] = useState<SizeUnit>(() =>
    mib > 0 && mib % SIZE_UNITS.GB === 0 ? "GB" : "MB"
  );
  const shown = Number.isNaN(mib)
    ? ""
    : Number((mib / SIZE_UNITS[unit]).toFixed(4));

  return (
    <Field>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <div className="flex gap-2">
        <Select
          disabled={disabled}
          onValueChange={(next) => onModeChange(next as ByteLimitMode)}
          value={mode}
        >
          <SelectTrigger className="w-full min-w-0" id={id}>
            {/* Without a render function this would show the raw value (inherit
               / unlimited / custom). */}
            <SelectValue>
              {(value: ByteLimitMode) =>
                value === "inherit" ? (inheritLabel ?? "") : labels[value]
              }
            </SelectValue>
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false}>
            {inheritLabel !== undefined && (
              <SelectItem value="inherit">{inheritLabel}</SelectItem>
            )}
            <SelectItem value="unlimited">{labels.unlimited}</SelectItem>
            <SelectItem value="custom">{labels.custom}</SelectItem>
          </SelectContent>
        </Select>
        {mode === "custom" && (
          <>
            <Input
              aria-label={m.roles_byte_limit_mib_aria({ label })}
              className="w-24 shrink-0 tabular-nums"
              disabled={disabled}
              min={0}
              onChange={(event) =>
                onMibChange(
                  Math.round(event.target.valueAsNumber * SIZE_UNITS[unit])
                )
              }
              step="any"
              type="number"
              value={shown}
            />
            <Select
              disabled={disabled}
              onValueChange={(next) => setUnit(next as SizeUnit)}
              value={unit}
            >
              <SelectTrigger
                aria-label={m.settings_number_unit_aria()}
                className="w-20 shrink-0"
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent alignItemWithTrigger={false}>
                <SelectItem value="MB">MB</SelectItem>
                <SelectItem value="GB">GB</SelectItem>
              </SelectContent>
            </Select>
          </>
        )}
      </div>
      <FieldDescription>
        {mode === "inherit" &&
          (inheritDescription ??
            m.roles_byte_limit_inherit_description({
              inheritLabel: inheritLabel ?? "",
            }))}
        {mode === "unlimited" &&
          (unlimitedDescription ?? m.roles_byte_limit_unlimited_description())}
        {mode === "custom" &&
          (customDescription ?? m.roles_byte_limit_custom_description())}
      </FieldDescription>
    </Field>
  );
}
