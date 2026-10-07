import { createFileRoute } from "@tanstack/react-router";
import { Separator } from "@tessera/ui/components/separator";

import { InterfaceThemeField } from "@/components/interface-theme-field";
import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { AppearanceSettingsForm } from "./-components/appearance-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/theme")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: ThemeSettingsRoute,
});

function ThemeSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_theme_description()}
      title={m.settings_theme_title()}
    >
      <div className="flex flex-col gap-6">
        <InterfaceThemeField />
        <Separator />
        <AppearanceSettingsForm />
      </div>
    </SettingsPage>
  );
}
