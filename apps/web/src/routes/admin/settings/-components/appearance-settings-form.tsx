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
 * All three option sets are functions, not module-level consts: the `m.xxx()` calls
 * must read the locale at render time, and a const would freeze it at first module load.
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
 * Instance appearance: `appearance.mode` / `appearance.theme` / `appearance.accent`.
 *
 * This is what signed-out visitors see, and what signed-in users get for anything they
 * have not chosen. The three values are written into `index.html`, so the first paint
 * has the right colors. Not the same as the "Theme" field on this page, which only
 * writes local storage.
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
   * Reads the form's current values, not the saved ones: the preview must show what a
   * choice looks like before saving.
   *
   * `useWatch` types every field as optional (it cannot know they are registered), so
   * it is layered over `defaultValues` for complete values, instead of defaulting each
   * of the three fields again in the preview.
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
          Palette and accent use the same pickers as the user's own preferences
          (`@tessera/ui`'s ThemePicker / AccentPicker): both choose from the same values,
          and seeing colors is faster than reading a dropdown.

          Wired through Controller instead of local state so `field.onChange` marks the
          field dirty in RHF, and only changed keys are sent (see useSettingsForm).
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
