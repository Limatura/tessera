import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";

import { i18n } from "#i18n";
import { expiryLabel } from "@/lib/format";
import type { Capabilities } from "@/lib/instance";

const INSTANCE_DEFAULT = "instance";

/** Lifetimes offered, in hours: an hour, a day, a week, a month. */
const EXPIRY_HOURS = [1, 24, 168, 720];

interface ExpirySelectProps {
  id: string;
  /** `null` for the instance's default, `0` for never. */
  value: number | null;
  onChange: (hours: number | null) => void;
  capabilities: Capabilities | undefined;
}

/**
 * Only lifetimes the instance accepts: none past its ceiling, and "never"
 * only when it has none.
 */
export function ExpirySelect({
  id,
  value,
  onChange,
  capabilities,
}: ExpirySelectProps) {
  const max = capabilities?.max_expiry_hours ?? null;
  const instanceDefault = capabilities?.default_expiry_hours;
  const options = [
    ...EXPIRY_HOURS.filter((hours) => max === null || hours <= max),
    ...(max === null ? [0] : []),
  ];

  const label = (selected: string) => {
    if (selected !== INSTANCE_DEFAULT) {
      return expiryLabel(Number(selected));
    }
    return instanceDefault === undefined
      ? i18n.t("expiry.instance")
      : i18n.t("expiry.instanceIs", [expiryLabel(instanceDefault ?? 0)]);
  };

  return (
    <Select
      onValueChange={(selected) =>
        onChange(
          selected === INSTANCE_DEFAULT || selected === null
            ? null
            : Number(selected)
        )
      }
      value={value === null ? INSTANCE_DEFAULT : String(value)}
    >
      <SelectTrigger className="min-w-44 shrink-0" id={id} size="sm">
        <SelectValue>{(selected: string) => label(selected)}</SelectValue>
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={INSTANCE_DEFAULT}>
          {label(INSTANCE_DEFAULT)}
        </SelectItem>
        {options.map((hours) => (
          <SelectItem key={hours} value={String(hours)}>
            {expiryLabel(hours)}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
