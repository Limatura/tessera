import { AccentPicker } from "@tessera/ui/components/theme-picker";

import { useAppearancePreference } from "@/hooks/use-appearance-preference";
import { APPEARANCE_ACCENTS } from "@/lib/appearance";
import * as m from "@/paraglide/messages";

/**
 * Accent color. Swatches are drawn by `AccentPicker` from `@tessera/ui`; each carries
 * its own `data-accent` and uses `bg-primary`, so it shows the color a click applies.
 *
 * Options come straight from `APPEARANCE_ACCENTS` instead of a local copy: that list
 * already has to match the backend's `Accent` enum and globals.css, and a third copy
 * is one more place to drift.
 */
export function AccentColorField() {
  const { choose, current, isPending } = useAppearancePreference();

  const options = APPEARANCE_ACCENTS.map((value) => ({
    label: m.user_prefs_accent_select({ color: value }),
    value,
  }));

  return (
    <div className="space-y-3">
      <div>
        <p className="font-medium text-sm">{m.user_prefs_accent_title()}</p>
        <p className="text-muted-foreground text-sm">
          {m.user_prefs_accent_description()}
        </p>
      </div>
      <AccentPicker
        disabled={isPending}
        onSelect={(accent) => choose({ accent })}
        options={options}
        value={current.accent}
      />
    </div>
  );
}
