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
  /** 從現在起幾分鐘;`undefined` 是永不過期 */
  minutes: number | undefined;
  /** 站台設定的保存上限(小時);有上限時「永不過期」會被後端拒絕,不提供 */
  maxHours: number | null | undefined;
  onChange: (minutes: number | undefined) => void;
}

/**
 * 分享結果卡片的過期時間:幾個常用的預設值,加上「自訂」展開一列輸入框。
 * 目前的值不在預設裡(站台預設 72 小時、或之前自訂過)時,照樣顯示成一個
 * 選項,不然 Select 會沒有對應的項目可以標成已選。
 *
 * 自訂的數字可以是小數(0.5 小時),只要換算出來是整數分鐘——後端的單位是分鐘。
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
            {/* step="any":預設的 step=1 會讓瀏覽器擋掉 0.5 */}
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

/** 時長用介面語言顯示(「7 天」而不是 10080),挑能整除的最大單位。 */
function formatMinutes(minutes: number): string {
  const unit =
    UNITS.toReversed().find((option) => minutes % MINUTES_PER[option] === 0) ??
    "minute";
  return new Intl.NumberFormat(getLocale(), {
    style: "unit",
    unit,
    unitDisplay: "long",
  }).format(minutes / MINUTES_PER[unit]);
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
