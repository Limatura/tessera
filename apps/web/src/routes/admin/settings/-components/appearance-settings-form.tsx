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

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import {
  APPEARANCE_ACCENTS,
  APPEARANCE_MODES,
  APPEARANCE_THEMES,
  appearanceSettingsSchema,
} from "@/lib/admin/settings-schema";
import type { AppearanceSettingsValues } from "@/lib/admin/settings-schema";
import { asEnum } from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { AppearancePreview } from "./appearance-preview";
import { SelectField } from "./form-fields";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];

const KEYS = {
  accent: "appearance.accent",
  mode: "appearance.mode",
  theme: "appearance.theme",
} satisfies SettingsKeyMap<AppearanceSettingsValues>;

/*
 * 三組選項都是函式而不是 module-level const —— 裡面的 `m.xxx()` 要在渲染當下讀
 * locale，包成 const 只會在模組第一次載入時凍住語系。
 */
function modeOptions() {
  return [
    { label: m.settings_appearance_form_mode_system(), value: "system" },
    { label: m.settings_appearance_form_mode_light(), value: "light" },
    { label: m.settings_appearance_form_mode_dark(), value: "dark" },
  ] as const;
}

function themeOptions() {
  return [
    {
      description: m.settings_appearance_form_theme_default_description(),
      name: m.settings_appearance_form_theme_default_name(),
      value: "default",
    },
    {
      description: m.settings_appearance_form_theme_graphite_description(),
      name: "Graphite",
      value: "graphite",
    },
    {
      description: m.settings_appearance_form_theme_sepia_description(),
      name: "Sepia",
      value: "sepia",
    },
    {
      description: m.settings_appearance_form_theme_matsu_description(),
      name: "Matsu",
      value: "matsu",
    },
    {
      description: m.settings_appearance_form_theme_cosmic_description(),
      name: "Cosmic",
      value: "cosmic",
    },
    {
      description: m.settings_appearance_form_theme_slate_description(),
      name: "Slate",
      value: "slate",
    },
    {
      description: m.settings_appearance_form_theme_vintage_description(),
      name: "Vintage",
      value: "vintage",
    },
  ] as const;
}

function accentOptions() {
  return [
    { label: m.settings_appearance_form_accent_violet(), value: "violet" },
    { label: m.settings_appearance_form_accent_rose(), value: "rose" },
    { label: m.settings_appearance_form_accent_orange(), value: "orange" },
    { label: m.settings_appearance_form_accent_amber(), value: "amber" },
    { label: m.settings_appearance_form_accent_green(), value: "green" },
    { label: m.settings_appearance_form_accent_teal(), value: "teal" },
    { label: m.settings_appearance_form_accent_blue(), value: "blue" },
  ] as const;
}

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
      "orange"
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
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      <SettingsCard
        description={m.settings_appearance_card_description()}
        title={m.settings_appearance_card_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="mode"
            render={({ field, fieldState }) => (
              <SelectField
                description={m.settings_appearance_form_mode_description()}
                field={field}
                fieldState={fieldState}
                id="appearance-mode"
                label={m.settings_appearance_form_mode_label()}
                options={modeOptions()}
              />
            )}
          />

          {/*
          色盤和強調色用跟前台個人偏好同一組選擇器（`@tessera/ui` 的 ThemePicker /
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
                  <FieldLabel>
                    {m.settings_appearance_form_theme_label()}
                  </FieldLabel>
                  <FieldDescription>
                    {m.settings_appearance_form_theme_description()}
                  </FieldDescription>
                </FieldContent>
                <ThemePicker
                  onSelect={field.onChange}
                  options={themeOptions()}
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
                  <FieldLabel>
                    {m.settings_appearance_form_accent_label()}
                  </FieldLabel>
                  <FieldDescription>
                    {m.settings_appearance_form_accent_description()}
                  </FieldDescription>
                </FieldContent>
                <AccentPicker
                  onSelect={field.onChange}
                  options={accentOptions()}
                  value={field.value}
                />
              </Field>
            )}
          />

          <AppearancePreview values={preview} />
        </FieldGroup>
      </SettingsCard>

      <SettingsFormFooter form={form} />
    </form>
  );
}
