import { IconDeviceDesktop, IconMoon, IconSun } from "@tabler/icons-react";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Tabs, TabsList, TabsTrigger } from "@tessera/ui/components/tabs";
import { useTheme } from "@tessera/ui/components/theme-provider";

import * as m from "@/paraglide/messages";

function themeOptions() {
  return [
    { icon: IconSun, label: m.settings_theme_field_light(), value: "light" },
    { icon: IconMoon, label: m.settings_theme_field_dark(), value: "dark" },
    {
      icon: IconDeviceDesktop,
      label: m.settings_theme_field_system(),
      value: "system",
    },
  ] as const;
}

/**
 * 這一項跟同頁的「訪客預設主題」不一樣：它走 next-themes，只寫這台瀏覽器的
 * localStorage，點下去立刻生效，所以不放進要按「儲存」的表單裡。
 */
export function InterfaceThemeField() {
  const { theme, setTheme } = useTheme();

  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel>{m.settings_theme_field_label()}</FieldLabel>
        <FieldDescription>
          {m.settings_theme_field_description()}
        </FieldDescription>
      </FieldContent>
      <Tabs onValueChange={setTheme} value={theme ?? "system"}>
        <TabsList>
          {themeOptions().map((option) => (
            <TabsTrigger key={option.value} value={option.value}>
              <option.icon />
              {option.label}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
    </Field>
  );
}
