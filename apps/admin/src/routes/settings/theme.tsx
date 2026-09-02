import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "../__root";
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
    <SettingsPage description="介面預設主題與主色調" title="介面主題">
      <ThemeSettingsForm />
    </SettingsPage>
  );
}
