import { createRoute } from "@tanstack/react-router";
import { Separator } from "@tessera/ui/components/separator";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { Route as rootRoute } from "../__root";
import { AppearanceSettingsForm } from "./-components/appearance-settings-form";
import { InterfaceThemeField } from "./-components/interface-theme-field";
import { SettingsPage } from "./-components/settings-page";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: ThemeSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/theme",
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
