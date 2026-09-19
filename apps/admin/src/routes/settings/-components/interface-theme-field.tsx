import { IconDeviceDesktop, IconMoon, IconSun } from "@tabler/icons-react";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Tabs, TabsList, TabsTrigger } from "@tessera/ui/components/tabs";
import { useTheme } from "@tessera/ui/components/theme-provider";

const THEME_OPTIONS = [
  { icon: IconSun, label: "淺色主題", value: "light" },
  { icon: IconMoon, label: "深色主題", value: "dark" },
  { icon: IconDeviceDesktop, label: "系統", value: "system" },
] as const;

/**
 * 這一項跟同頁的「訪客預設主題」不一樣：它走 next-themes，只寫這台瀏覽器的
 * localStorage，點下去立刻生效，所以不放進要按「儲存」的表單裡。
 */
export function InterfaceThemeField() {
  const { theme, setTheme } = useTheme();

  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel>主題</FieldLabel>
        <FieldDescription>主題設定僅為目前使用者儲存。</FieldDescription>
      </FieldContent>
      <Tabs onValueChange={setTheme} value={theme ?? "system"}>
        <TabsList>
          {THEME_OPTIONS.map((option) => (
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
