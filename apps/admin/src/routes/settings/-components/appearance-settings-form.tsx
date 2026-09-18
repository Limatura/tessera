import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup } from "@tessera/ui/components/field";
import { Controller } from "react-hook-form";

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import {
  APPEARANCE_ACCENTS,
  APPEARANCE_MODES,
  APPEARANCE_THEMES,
  appearanceSettingsSchema,
} from "@/lib/settings-schema";
import type { AppearanceSettingsValues } from "@/lib/settings-schema";
import { asEnum } from "@/lib/settings-values";

import { SelectField } from "./form-fields";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const KEYS = {
  accent: "appearance.accent",
  mode: "appearance.mode",
  theme: "appearance.theme",
} satisfies SettingsKeyMap<AppearanceSettingsValues>;

const MODE_OPTIONS = [
  { label: "跟隨裝置", value: "system" },
  { label: "淺色", value: "light" },
  { label: "深色", value: "dark" },
] as const;

const THEME_OPTIONS = [
  { label: "預設", value: "default" },
  { label: "Graphite（偏冷、較平）", value: "graphite" },
  { label: "Sepia（暖如紙）", value: "sepia" },
] as const;

const ACCENT_OPTIONS = [
  { label: "紫 Violet", value: "violet" },
  { label: "玫瑰 Rose", value: "rose" },
  { label: "橘 Orange", value: "orange" },
  { label: "琥珀 Amber", value: "amber" },
  { label: "綠 Green", value: "green" },
  { label: "青 Teal", value: "teal" },
  { label: "藍 Blue", value: "blue" },
] as const;

/**
 * 站台外觀：`appearance.mode` / `appearance.theme` / `appearance.accent`。
 *
 * 講的是 signed-out 訪客看到的樣子，以及登入者沒自己選過的部分。三個值會寫進
 * `index.html`，所以首次繪製就是對的顏色 —— 跟同一頁那個只寫本機 localStorage 的
 * 「主題」不是同一件事。
 */
export function AppearanceSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <AppearanceSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): AppearanceSettingsValues {
  return {
    accent: asEnum(
      readSetting(settings, KEYS.accent),
      APPEARANCE_ACCENTS,
      "violet"
    ),
    mode: asEnum(readSetting(settings, KEYS.mode), APPEARANCE_MODES, "system"),
    theme: asEnum(
      readSetting(settings, KEYS.theme),
      APPEARANCE_THEMES,
      "default"
    ),
  };
}

function AppearanceSettingsFields({
  defaultValues,
}: {
  defaultValues: AppearanceSettingsValues;
}) {
  const form = useSettingsForm<AppearanceSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(appearanceSettingsSchema),
  });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <Controller
          control={form.control}
          name="mode"
          render={({ field, fieldState }) => (
            <SelectField
              description="訪客首次造訪時套用的明暗，之後仍可自行切換。跟隨裝置時讀 prefers-color-scheme。"
              field={field}
              fieldState={fieldState}
              id="appearance-mode"
              label="預設明暗"
              options={MODE_OPTIONS}
            />
          )}
        />

        <Controller
          control={form.control}
          name="theme"
          render={({ field, fieldState }) => (
            <SelectField
              description="背景、文字與邊框用的中性色盤。"
              field={field}
              fieldState={fieldState}
              id="appearance-theme"
              label="色盤"
              options={THEME_OPTIONS}
            />
          )}
        />

        <Controller
          control={form.control}
          name="accent"
          render={({ field, fieldState }) => (
            <SelectField
              description="按鈕、連結與 focus ring 的強調色。每個顏色都是 packages/ui 裡的一段 CSS，所以只能從這七個選。"
              field={field}
              fieldState={fieldState}
              id="appearance-accent"
              label="強調色"
              options={ACCENT_OPTIONS}
            />
          )}
        />

        <SettingsFormFooter
          canWrite={form.canWrite}
          errorMessage={form.errorMessage}
          isDirty={form.isDirty}
          isPending={form.isPending}
          isSaved={form.isSaved}
        />
      </FieldGroup>
    </form>
  );
}
