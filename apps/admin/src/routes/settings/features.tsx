import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "../__root";
import { FeatureSettingsForm } from "./-components/feature-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: FeatureSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/features",
});

function FeatureSettingsRoute() {
  return (
    <SettingsPage description="執行期開關與自訂連結預覽卡片" title="功能開關">
      <FeatureSettingsForm />
    </SettingsPage>
  );
}
