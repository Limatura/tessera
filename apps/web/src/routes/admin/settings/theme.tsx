import { createFileRoute } from "@tanstack/react-router";

import { InterfaceThemeField } from "@/components/interface-theme-field";
import * as m from "@/paraglide/messages";

import { AppearanceSettingsForm } from "./-components/appearance-settings-form";
import { SettingsCard, SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/theme")({
  component: ThemeSettingsRoute,
  staticData: { title: m.nav_settings_theme },
});

/**
 * Two different things on one page: on top, how this panel looks in your own
 * browser (a personal preference, not saved, affects no one else); below, the
 * site settings. Separate cards, so no one thinks the top one needs saving.
 */
function ThemeSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_theme_description()}
      title={m.settings_theme_title()}
    >
      <AppearanceSettingsForm />
      <SettingsCard
        description={m.settings_theme_personal_description()}
        title={m.settings_theme_personal_title()}
      >
        <InterfaceThemeField />
      </SettingsCard>
    </SettingsPage>
  );
}
