import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import { AccentPicker, ThemePicker } from "@tessera/ui/components/theme-picker";
import { Controller, useWatch } from "react-hook-form";

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

import { AppearancePreview } from "./appearance-preview";
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
  { description: "這份設計本來的樣子。", name: "預設", value: "default" },
  { description: "偏冷、較平的中性灰。", name: "Graphite", value: "graphite" },
  { description: "暖如紙，長時間閱讀舒適。", name: "Sepia", value: "sepia" },
  {
    description: "暖米紙配橄欖綠，柔和手繪調。",
    name: "Matsu",
    value: "matsu",
  },
  {
    description: "深藍紫夜空，深色模式最出色。",
    name: "Cosmic",
    value: "cosmic",
  },
  { description: "乾淨的冷灰，最不干擾內容。", name: "Slate", value: "slate" },
  {
    description: "低對比茶漬紙，像舊印刷品。",
    name: "Vintage",
    value: "vintage",
  },
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

  /*
   * 讀表單當下的值,而不是已儲存的 —— 預覽要在按儲存之前就答得出「選這組長怎樣」。
   *
   * `useWatch` 的回傳型別每個欄位都是選填的(它不知道欄位一定已經註冊),所以
   * 疊在 `defaultValues` 上湊成完整值,而不是在預覽裡對三個欄位各補一次預設。
   */
  const watched = useWatch({ control: form.control });
  const preview = { ...defaultValues, ...watched };

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

        {/*
          色盤和強調色用跟 apps/web 同一組選擇器（`@tessera/ui` 的 ThemePicker /
          AccentPicker）：兩邊選的是同一組值，看得到顏色也比讀下拉選單的文字快。

          走 Controller 而不是自己接 state，`field.onChange` 才會讓 RHF 記下
          dirty —— 送出時只送真正改過的 key（見 useSettingsForm）。
        */}
        <Controller
          control={form.control}
          name="theme"
          render={({ field }) => (
            <Field>
              <FieldContent>
                <FieldLabel>色盤</FieldLabel>
                <FieldDescription>
                  背景、文字與邊框用的中性色盤。
                </FieldDescription>
              </FieldContent>
              <ThemePicker
                onSelect={field.onChange}
                options={THEME_OPTIONS}
                value={field.value}
              />
            </Field>
          )}
        />

        <Controller
          control={form.control}
          name="accent"
          render={({ field }) => (
            <Field>
              <FieldContent>
                <FieldLabel>強調色</FieldLabel>
                <FieldDescription>
                  按鈕、連結與 focus ring 的強調色。每個顏色都是 packages/ui
                  裡的一段 CSS，所以只能從這七個選。
                </FieldDescription>
              </FieldContent>
              <AccentPicker
                onSelect={field.onChange}
                options={ACCENT_OPTIONS}
                value={field.value}
              />
            </Field>
          )}
        />

        {/* 三個值是一起看的,所以預覽讀表單當下的值而不是已儲存的值。 */}
        <AppearancePreview values={preview} />

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
