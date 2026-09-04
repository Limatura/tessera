import { IconCheck } from "@tabler/icons-react";
import { cn } from "@tessera/ui/lib/utils";

import * as m from "@/paraglide/messages";
import type { AccentColor } from "@/stores/appearance";
import { useAppearanceStore } from "@/stores/appearance";

/** 色票只用來顯示,選了什麼跟 AppThemeField 一樣先只存起來。 */
const SWATCHES: { value: AccentColor; className: string }[] = [
  { className: "bg-violet-500", value: "violet" },
  { className: "bg-rose-500", value: "rose" },
  { className: "bg-orange-500", value: "orange" },
  { className: "bg-amber-500", value: "amber" },
  { className: "bg-green-500", value: "green" },
  { className: "bg-teal-500", value: "teal" },
  { className: "bg-blue-500", value: "blue" },
];

export function AccentColorField() {
  const accentColor = useAppearanceStore((state) => state.accentColor);
  const setAccentColor = useAppearanceStore((state) => state.setAccentColor);

  return (
    <div className="space-y-3">
      <div>
        <p className="font-medium text-sm">{m.user_prefs_accent_title()}</p>
        <p className="text-muted-foreground text-sm">
          {m.user_prefs_accent_description()}
        </p>
      </div>
      <div className="flex flex-wrap gap-2">
        {SWATCHES.map((swatch) => (
          <button
            aria-label={m.user_prefs_accent_select({ color: swatch.value })}
            aria-pressed={accentColor === swatch.value}
            className={cn(
              "flex size-7 items-center justify-center rounded-full text-white ring-offset-2 ring-offset-background transition-shadow focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              swatch.className,
              accentColor === swatch.value && "ring-2 ring-foreground"
            )}
            key={swatch.value}
            onClick={() => setAccentColor(swatch.value)}
            type="button"
          >
            {accentColor === swatch.value ? <IconCheck size={14} /> : null}
          </button>
        ))}
      </div>
    </div>
  );
}
