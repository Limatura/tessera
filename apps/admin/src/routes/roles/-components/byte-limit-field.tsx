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

/**
 * 配額欄位有三種狀態，而且 `null` 跟 `0` 是不同的意思：
 * `null` 跟著全站設定跑（全站改了它就跟著改），`0` 是「這個角色不設上限」。
 * 用一個數字輸入框表達不出這個差別，所以拆成模式 + 數值。
 */
const MIB = 1024 * 1024;

export type ByteLimitMode = "inherit" | "unlimited" | "custom";

/** `inherit` 的字樣每個欄位不同，由呼叫端給。 */
const MODE_LABELS: Record<ByteLimitMode, string> = {
  custom: "自訂",
  inherit: "跟隨全站",
  unlimited: "不限制",
};

export function modeOf(value: number | null | undefined): ByteLimitMode {
  if (value === null || value === undefined) {
    return "inherit";
  }
  return value === 0 ? "unlimited" : "custom";
}

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
}: {
  id: string;
  label: string;
  inheritLabel: string;
  mode: ByteLimitMode;
  mib: number;
  onModeChange: (mode: ByteLimitMode) => void;
  onMibChange: (mib: number) => void;
  disabled?: boolean;
}) {
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
            {/* 不給對應函式的話會顯示原始值（inherit / unlimited / custom）。 */}
            <SelectValue>
              {(value: ByteLimitMode) =>
                value === "inherit" ? inheritLabel : MODE_LABELS[value]
              }
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="inherit">{inheritLabel}</SelectItem>
            <SelectItem value="unlimited">{MODE_LABELS.unlimited}</SelectItem>
            <SelectItem value="custom">{MODE_LABELS.custom}</SelectItem>
          </SelectContent>
        </Select>
        {mode === "custom" && (
          <Input
            aria-label={`${label}（MiB）`}
            className="w-24 shrink-0"
            disabled={disabled}
            min={1}
            onChange={(event) => onMibChange(event.target.valueAsNumber)}
            type="number"
            value={Number.isNaN(mib) ? "" : mib}
          />
        )}
      </div>
      <FieldDescription>
        {mode === "inherit" && `${inheritLabel}，全站設定改了會跟著改`}
        {mode === "unlimited" && "這個角色不套用自己的上限"}
        {mode === "custom" && "單位為 MiB"}
      </FieldDescription>
    </Field>
  );
}
