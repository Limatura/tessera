import { useTheme } from "next-themes";

import { useAppearancePreference } from "@/hooks/use-appearance-preference";
import type { AppearanceMode } from "@/lib/appearance";

/**
 * Light/dark from the admin header and command menu, written the same two
 * places `InterfaceThemeField` writes: next-themes for an instant switch, the
 * server so another device follows. Everyone in the panel is signed in, so the
 * preference endpoint is always there.
 */
export function useThemeMode() {
  const { resolvedTheme, setTheme } = useTheme();
  const { choose } = useAppearancePreference();

  const setMode = (mode: AppearanceMode) => {
    setTheme(mode);
    choose({ mode });
  };

  return {
    resolved: resolvedTheme === "dark" ? ("dark" as const) : ("light" as const),
    setMode,
  };
}
