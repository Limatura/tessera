import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@tessera/ui/components/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { Separator } from "@tessera/ui/components/separator";

import { getLocale, setLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";
import { locales } from "@/paraglide/runtime.js";
import type { Locale } from "@/paraglide/runtime.js";
import type { TimeFormat } from "@/stores/appearance";
import { useAppearanceStore } from "@/stores/appearance";

import { AccentColorField } from "./accent-color-field";
import { AppThemeField } from "./app-theme-field";
import { InterfaceThemeField } from "./interface-theme-field";

const LOCALE_LABELS: Record<Locale, string> = {
  en: "English",
  ja: "日本語",
  "zh-TW": "繁體中文",
};

export function PreferencesTab() {
  return (
    <div className="space-y-8">
      <div className="space-y-4">
        <h2 className="font-medium text-muted-foreground text-sm">
          {m.user_prefs_general()}
        </h2>
        <LanguageField />
        <TimeFormatField />
      </div>

      <Separator />

      <div className="space-y-6">
        <h2 className="font-medium text-muted-foreground text-sm">
          {m.user_prefs_appearance()}
        </h2>
        <InterfaceThemeField />
        <AppThemeField />
        <AccentColorField />
      </div>
    </div>
  );
}

/** 換語言會讓 __root remount(key={locale}),所以這裡不需要自己做任何事。 */
function LanguageField() {
  const locale = getLocale();

  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel>{m.user_prefs_language_title()}</FieldLabel>
        <FieldDescription>
          {m.user_prefs_language_description()}
        </FieldDescription>
      </FieldContent>
      <Select
        onValueChange={(value) => setLocale(value as Locale)}
        value={locale}
      >
        <SelectTrigger className="w-40">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {locales.map((available) => (
              <SelectItem key={available} value={available}>
                {LOCALE_LABELS[available]}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </Field>
  );
}

function TimeFormatField() {
  const timeFormat = useAppearanceStore((state) => state.timeFormat);
  const setTimeFormat = useAppearanceStore((state) => state.setTimeFormat);

  const labels: Record<TimeFormat, string> = {
    "12h": m.user_prefs_time_format_12h(),
    "24h": m.user_prefs_time_format_24h(),
    auto: m.user_prefs_time_format_auto(),
  };

  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel>{m.user_prefs_time_format_title()}</FieldLabel>
        <FieldDescription>
          {m.user_prefs_time_format_description()}
        </FieldDescription>
      </FieldContent>
      <Select
        onValueChange={(value) => setTimeFormat(value as TimeFormat)}
        value={timeFormat}
      >
        <SelectTrigger className="w-40">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {Object.entries(labels).map(([value, label]) => (
              <SelectItem key={value} value={value}>
                {label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </Field>
  );
}
