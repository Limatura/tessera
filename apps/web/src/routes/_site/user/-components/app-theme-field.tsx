import { ThemePicker } from "@tessera/ui/components/theme-picker";

import { useAppearancePreference } from "@/hooks/use-appearance-preference";
import type { AppTheme } from "@/lib/appearance";
import * as m from "@/paraglide/messages";

/**
 * The overall palette. A choice is saved on the server, and `AppearanceSync` writes
 * `data-theme` back to `<html>`; the CSS is in globals.css in `packages/ui`.
 *
 * The picker is the shared `@tessera/ui` component the admin panel also uses for the
 * instance default. Both choose from the same values, and looking different would
 * suggest they are two different things.
 */
export function AppThemeField() {
  const { choose, current, isPending } = useAppearancePreference();

  const options: { value: AppTheme; name: string; description: string }[] = [
    {
      description: m.user_prefs_app_theme_default_description(),
      name: m.user_prefs_app_theme_default_name(),
      value: "default",
    },
    {
      description: m.user_prefs_app_theme_graphite_description(),
      name: m.user_prefs_app_theme_graphite_name(),
      value: "graphite",
    },
    {
      description: m.user_prefs_app_theme_sepia_description(),
      name: m.user_prefs_app_theme_sepia_name(),
      value: "sepia",
    },
    {
      description: m.user_prefs_app_theme_matsu_description(),
      name: m.user_prefs_app_theme_matsu_name(),
      value: "matsu",
    },
    {
      description: m.user_prefs_app_theme_cosmic_description(),
      name: m.user_prefs_app_theme_cosmic_name(),
      value: "cosmic",
    },
    {
      description: m.user_prefs_app_theme_slate_description(),
      name: m.user_prefs_app_theme_slate_name(),
      value: "slate",
    },
    {
      description: m.user_prefs_app_theme_vintage_description(),
      name: m.user_prefs_app_theme_vintage_name(),
      value: "vintage",
    },
  ];

  return (
    <div className="space-y-3">
      <div>
        <p className="font-medium text-sm">{m.user_prefs_app_theme_title()}</p>
        <p className="text-muted-foreground text-sm">
          {m.user_prefs_app_theme_description()}
        </p>
      </div>
      <ThemePicker
        disabled={isPending}
        onSelect={(theme) => choose({ theme })}
        options={options}
        value={current.theme}
      />
    </div>
  );
}
