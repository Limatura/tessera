import { ThemePicker } from "@tessera/ui/components/theme-picker";

import type { AppTheme } from "@/lib/appearance";
import * as m from "@/paraglide/messages";

import { useAppearancePreference } from "./use-appearance-preference";

/**
 * 整體色盤。選了就存到伺服器,`AppearanceSync` 會把 `data-theme` 寫回 `<html>`,
 * 對應的 CSS 在 `packages/ui` 的 globals.css 裡。
 *
 * 選擇器本身是 `@tessera/ui` 的共用元件,後台選站台預設時用的是同一個 —— 兩邊
 * 選的是同一組值,長得不一樣只會讓人以為它們是兩件事。
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
