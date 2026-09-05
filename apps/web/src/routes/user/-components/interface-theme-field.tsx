import { IconDeviceDesktop, IconMoon, IconSun } from "@tabler/icons-react";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Tabs, TabsList, TabsTrigger } from "@tessera/ui/components/tabs";

import { useTheme } from "@/components/theme-provider";
import * as m from "@/paraglide/messages";

/** 走 next-themes,只寫這台瀏覽器的 localStorage,點下去立刻生效。 */
export function InterfaceThemeField() {
  const { theme, setTheme } = useTheme();

  const options = [
    { icon: IconSun, label: m.user_prefs_theme_light(), value: "light" },
    { icon: IconMoon, label: m.user_prefs_theme_dark(), value: "dark" },
    {
      icon: IconDeviceDesktop,
      label: m.user_prefs_theme_system(),
      value: "system",
    },
  ];

  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel>{m.user_prefs_theme_title()}</FieldLabel>
        <FieldDescription>{m.user_prefs_theme_description()}</FieldDescription>
      </FieldContent>
      <Tabs
        onValueChange={(value) => setTheme(value as string)}
        value={theme ?? "system"}
      >
        <TabsList>
          {options.map((option) => (
            <TabsTrigger key={option.value} value={option.value}>
              <option.icon size={16} />
              {option.label}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
    </Field>
  );
}
