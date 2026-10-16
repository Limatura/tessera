import { IconDeviceDesktop, IconMoon, IconSun } from "@tabler/icons-react";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Tabs, TabsList, TabsTrigger } from "@tessera/ui/components/tabs";
import { useTheme } from "next-themes";

import { useAppearancePreference } from "@/hooks/use-appearance-preference";
import type { AppearanceMode } from "@/lib/appearance";
import * as m from "@/paraglide/messages";

/**
 * Light/dark. Both places are written:
 *
 * - next-themes swaps the `.dark` class at once, so the change is visible without a round trip
 * - the server stores it, so another device gets the same mode
 *
 * The displayed value is the one resolved from the server, not
 * `useTheme().theme`, or right after signing in it would show the choice the
 * previous user left in this browser.
 */
export function InterfaceThemeField() {
  const { choose, current, isPending } = useAppearancePreference();
  const { setTheme } = useTheme();

  const options = [
    { icon: IconSun, label: m.user_prefs_theme_light(), value: "light" },
    { icon: IconMoon, label: m.user_prefs_theme_dark(), value: "dark" },
    {
      icon: IconDeviceDesktop,
      label: m.user_prefs_theme_system(),
      value: "system",
    },
  ] as const;

  const handleChange = (value: unknown) => {
    const mode = value as AppearanceMode;
    setTheme(mode);
    choose({ mode });
  };

  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel>{m.user_prefs_theme_title()}</FieldLabel>
        <FieldDescription>{m.user_prefs_theme_description()}</FieldDescription>
      </FieldContent>
      <Tabs onValueChange={handleChange} value={current.mode}>
        <TabsList>
          {options.map((option) => (
            <TabsTrigger
              disabled={isPending}
              key={option.value}
              value={option.value}
            >
              <option.icon size={16} />
              {option.label}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
    </Field>
  );
}
