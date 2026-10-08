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

import * as m from "@/paraglide/messages";

/**
 * 配額欄位有三種狀態，而且 `null` 跟 `0` 是不同的意思：
 * `null` 跟著上一層跑（角色跟全站、帳號跟角色；上一層改了它就跟著改），`0` 是
 * 「這一層不設上限」。用一個數字輸入框表達不出這個差別，所以拆成模式 + 數值。
 */
const MIB = 1024 * 1024;

export type ByteLimitMode = "inherit" | "unlimited" | "custom";

/** `inherit` 的字樣每個欄位不同，由呼叫端給。呼叫時才讀取，語系才能即時反映。 */
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

/** `null` = 清掉；`0` = 不限制。 */
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
}: {
  id: string;
  label: string;
  inheritLabel: string;
  /** 「沿用」時的說明。預設講的是全站，帳號那邊沿用的是角色。 */
  inheritDescription?: string;
  mode: ByteLimitMode;
  mib: number;
  onModeChange: (mode: ByteLimitMode) => void;
  onMibChange: (mib: number) => void;
  disabled?: boolean;
}) {
  const labels = modeLabels();

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
                value === "inherit" ? inheritLabel : labels[value]
              }
            </SelectValue>
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false}>
            <SelectItem value="inherit">{inheritLabel}</SelectItem>
            <SelectItem value="unlimited">{labels.unlimited}</SelectItem>
            <SelectItem value="custom">{labels.custom}</SelectItem>
          </SelectContent>
        </Select>
        {mode === "custom" && (
          <Input
            aria-label={m.roles_byte_limit_mib_aria({ label })}
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
        {mode === "inherit" &&
          (inheritDescription ??
            m.roles_byte_limit_inherit_description({ inheritLabel }))}
        {mode === "unlimited" && m.roles_byte_limit_unlimited_description()}
        {mode === "custom" && m.roles_byte_limit_custom_description()}
      </FieldDescription>
    </Field>
  );
}
