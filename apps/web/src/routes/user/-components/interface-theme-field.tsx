import { IconDeviceDesktop, IconMoon, IconSun } from "@tabler/icons-react";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Tabs, TabsList, TabsTrigger } from "@tessera/ui/components/tabs";
import { useTheme } from "next-themes";

import type { AppearanceMode } from "@/lib/appearance";
import * as m from "@/paraglide/messages";

import { useAppearancePreference } from "./use-appearance-preference";

/**
 * 明暗。兩邊都要寫:
 *
 * - next-themes 立刻換掉 `.dark` class,所以點下去就看到結果,不用等 round trip
 * - 伺服器存起來,所以換一台裝置還是同一個明暗
 *
 * 讀的時候以伺服器解出來的值為準而不是 `useTheme().theme`,不然剛登入時會顯示
 * 上一個人在這台瀏覽器留下的選擇。
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
