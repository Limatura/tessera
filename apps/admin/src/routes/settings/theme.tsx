import { createRoute } from "@tanstack/react-router";
import { Separator } from "@tessera/ui/components/separator";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "../__root";
import { InterfaceThemeField } from "./-components/interface-theme-field";
import { SettingsPage } from "./-components/settings-page";
import { ThemeSettingsForm } from "./-components/theme-settings-form";

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
    <SettingsPage description="介面外觀與主色調" title="外觀">
      <div className="flex flex-col gap-6">
        <InterfaceThemeField />
        <Separator />
        <ThemeSettingsForm />
      </div>
    </SettingsPage>
  );
}
