import { IconClock } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { Input } from "@tessera/ui/components/input";
import { Label } from "@tessera/ui/components/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { useState } from "react";

import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

type Unit = "minute" | "hour" | "day";
const MINUTES_PER: Record<Unit, number> = { day: 24 * 60, hour: 60, minute: 1 };
const UNITS = ["minute", "hour", "day"] as const;

const PRESET_MINUTES = [30, 60, 24 * 60, 7 * 24 * 60] as const;
const NEVER = "0";
const CUSTOM = "custom";

interface ShareExpiryFieldProps {
  /** Minutes from now; `undefined` never expires. */
  minutes: number | undefined;
  /**
   * The site's retention limit in hours. With a limit the server refuses "never expires",
   * so it is not offered.
   */
  maxHours: number | null | undefined;
  onChange: (minutes: number | undefined) => void;
}

/**
 * The expiry on the share result card: a few common presets, plus "custom"
 * which opens a row of inputs. A current value that is not a preset (the site
 * default of 72 hours, or an earlier custom value) still shows as an option,
 * or the Select would have nothing to mark as selected.
 *
 * A custom number may be fractional (0.5 hours) as long as it comes to whole
 * minutes, the server's unit.
 */
export function ShareExpiryField({
  maxHours,
  minutes,
  onChange,
}: ShareExpiryFieldProps) {
  const [customOpen, setCustomOpen] = useState(false);
  const [amount, setAmount] = useState("");
  const [unit, setUnit] = useState<Unit>("hour");

  const maxMinutes = maxHours ? maxHours * MINUTES_PER.hour : undefined;
  const presets = PRESET_MINUTES.filter(
    (preset) => !maxMinutes || preset <= maxMinutes
  );
  const options = [
    ...presets,
    ...(minutes === undefined || presets.some((preset) => preset === minutes)
      ? []
      : [minutes]),
  ].toSorted((a, b) => a - b);

  const customMinutes = Number(amount) * MINUTES_PER[unit];
  const customValid = Number.isInteger(customMinutes) && customMinutes >= 1;
  const customTooLong = Boolean(maxMinutes && customMinutes > maxMinutes);
  const customInvalid = amount !== "" && !customValid;

  const handleSelect = (value: string | null) => {
    if (value === CUSTOM) {
      setCustomOpen(true);
      return;
    }
    setCustomOpen(false);
    const next = Number(value ?? NEVER);
    onChange(next === 0 ? undefined : next);
  };

  const handleApply = (event: React.FormEvent) => {
    event.preventDefault();
    if (!customValid || customTooLong) {
      return;
    }
    onChange(customMinutes);
    setCustomOpen(false);
    setAmount("");
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <Label className="text-current" htmlFor="share-result-expiry">
          <IconClock className="size-3.5" />
          {m.upload_settings_expiry_label()}
        </Label>
        <Select onValueChange={handleSelect} value={String(minutes ?? NEVER)}>
          <SelectTrigger id="share-result-expiry" size="sm">
            <SelectValue>
              {(value: string) =>
                value === NEVER
                  ? m.upload_settings_expiry_never()
                  : formatMinutes(Number(value))
              }
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {options.map((option) => (
              <SelectItem key={option} value={String(option)}>
                {formatMinutes(option)}
              </SelectItem>
            ))}
            {!maxHours && (
              <SelectItem value={NEVER}>
                {m.upload_settings_expiry_never()}
              </SelectItem>
            )}
            <SelectItem value={CUSTOM}>
              {m.upload_settings_expiry_custom()}
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      {customOpen && (
        <form className="flex flex-col gap-1" onSubmit={handleApply}>
          <div className="flex items-center gap-2">
            {/* step="any": the default step=1 makes the browser reject 0.5 */}
            <Input
              aria-invalid={customInvalid || customTooLong}
              aria-label={m.upload_settings_expiry_custom_amount()}
              autoFocus
              className="h-7 flex-1"
              inputMode="decimal"
              onChange={(event) => setAmount(event.target.value)}
              step="any"
              type="number"
              value={amount}
            />
            <Select
              onValueChange={(value) =>
                setUnit((value as Unit | null) ?? "hour")
              }
              value={unit}
            >
              <SelectTrigger
                aria-label={m.upload_settings_expiry_custom_unit()}
                size="sm"
              >
                <SelectValue>{(value: Unit) => unitLabel(value)}</SelectValue>
              </SelectTrigger>
              <SelectContent>
                {UNITS.map((option) => (
                  <SelectItem key={option} value={option}>
                    {unitLabel(option)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button
              className="shrink-0 border-current/25 text-current hover:bg-current/10 hover:text-current"
              disabled={!customValid || customTooLong}
              size="sm"
              type="submit"
              variant="outline"
            >
              {m.upload_settings_expiry_apply()}
            </Button>
          </div>
          {customInvalid && (
            <p className="text-destructive text-xs">
              {m.upload_settings_expiry_invalid()}
            </p>
          )}
          {customTooLong && maxMinutes && (
            <p className="text-destructive text-xs">
              {m.upload_settings_expiry_too_long({
                max: formatMinutes(maxMinutes),
              })}
            </p>
          )}
        </form>
      )}
    </div>
  );
}

/**
 * A duration in the interface language ("7 days", not 10080), in the largest
 * unit that divides it evenly.
 *
 * When settings are changed later, the time left almost never divides evenly
 * (2879 minutes), so the largest unit is used with one decimal and a leading
 * ≈. Showing "2,879 minutes" would leave the conversion to the reader.
 */
function formatMinutes(minutes: number): string {
  const exact = UNITS.toReversed().find(
    (option) => minutes % MINUTES_PER[option] === 0
  );
  if (exact && (exact !== "minute" || minutes < MINUTES_PER.hour)) {
    return unitFormat(exact).format(minutes / MINUTES_PER[exact]);
  }
  const unit =
    UNITS.toReversed().find((option) => minutes >= MINUTES_PER[option]) ??
    "minute";
  return `≈ ${unitFormat(unit).format(minutes / MINUTES_PER[unit])}`;
}

function unitFormat(unit: Unit): Intl.NumberFormat {
  return new Intl.NumberFormat(getLocale(), {
    maximumFractionDigits: 1,
    style: "unit",
    unit,
    unitDisplay: "long",
  });
}

function unitLabel(unit: Unit): string {
  if (unit === "day") {
    return m.upload_settings_expiry_unit_days();
  }
  if (unit === "hour") {
    return m.upload_settings_expiry_unit_hours();
  }
  return m.upload_settings_expiry_unit_minutes();
}
