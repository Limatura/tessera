import { createFileRoute } from "@tanstack/react-router";
import { Separator } from "@tessera/ui/components/separator";

import { InterfaceThemeField } from "@/components/interface-theme-field";
import * as m from "@/paraglide/messages";

import { AppearanceSettingsForm } from "./-components/appearance-settings-form";
import { SettingsCard } from "./-components/settings-card";

export const Route = createFileRoute("/admin/settings/theme")({
  component: ThemeSettingsRoute,
});

function ThemeSettingsRoute() {
  return (
    <SettingsCard
      description={m.settings_theme_description()}
      title={m.settings_theme_title()}
    >
      <div className="flex flex-col gap-6">
        <InterfaceThemeField />
        <Separator />
        <AppearanceSettingsForm />
      </div>
    </SettingsCard>
  );
}
