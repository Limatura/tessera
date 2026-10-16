import { cn } from "@tessera/ui/lib/utils";

import type { AppearanceSettingsValues } from "@/lib/admin/settings-schema";
import * as m from "@/paraglide/messages";

/**
 * A function, not a module-level const: the `m.xxx()` calls inside must read the
 * locale at render time, and a const would freeze it at first module load.
 */
function modeLabels(): Record<AppearanceSettingsValues["mode"], string> {
  return {
    dark: m.settings_appearance_preview_mode_dark(),
    light: m.settings_appearance_preview_mode_light(),
    system: m.settings_appearance_preview_mode_system(),
  };
}

/**
 * What the form's three current values look like together.
 *
 * Applied only to this div, not the admin panel: this page sets what visitors see, not
 * the operator's own theme (the next-themes field above it). Applying it to the panel
 * would restyle the operator's interface on every pick and blur the two. `data-theme`
 * and `data-accent` can affect just a subtree because globals.css uses attribute
 * selectors, not `:root`.
 *
 * Only half of light and dark can be previewed: dark comes from a `.dark` ancestor and
 * `@custom-variant dark (&:is(.dark *))`, which a descendant selector cannot undo, so
 * while the panel is dark this subtree cannot turn light. Copying the light tokens to
 * work around it would add a list that drifts from globals.css, so light previews only
 * when the panel is already light, and the text below says what was picked. Palette
 * and accent are unaffected, and they are what this page really chooses.
 */
export function AppearancePreview({
  values,
}: {
  values: AppearanceSettingsValues;
}) {
  return (
    <div
      className={cn(
        "rounded-lg border bg-background p-4 text-foreground",
        values.mode === "dark" && "dark"
      )}
      data-accent={values.accent}
      data-theme={values.theme}
    >
      <p className="mb-3 font-medium text-muted-foreground text-xs">
        {m.settings_appearance_preview_heading({
          mode: modeLabels()[values.mode],
        })}
      </p>
      <div className="rounded-md border bg-card p-3 text-card-foreground">
        <p className="font-medium text-sm">
          {m.settings_appearance_preview_card_title()}
        </p>
        <p className="mt-1 text-muted-foreground text-xs">
          {m.settings_appearance_preview_card_description()}
        </p>
        <div className="mt-3 flex items-center gap-2">
          <span className="rounded-md bg-primary px-3 py-1.5 font-medium text-primary-foreground text-xs">
            {m.settings_appearance_preview_download()}
          </span>
          <span className="rounded-md border px-3 py-1.5 text-xs">
            {m.settings_appearance_preview_cancel()}
          </span>
          <span className="ml-auto size-4 rounded-full bg-primary" />
        </div>
      </div>
    </div>
  );
}
