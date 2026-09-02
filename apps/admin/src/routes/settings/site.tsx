import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "../__root";
import { SettingsPage } from "./-components/settings-page";
import { SiteSettingsForm } from "./-components/site-settings-form";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: SiteSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/site",
});

function SiteSettingsRoute() {
  return (
    <SettingsPage description="網頁標題、敘述與 SEO 關鍵詞" title="網站資訊">
      <SiteSettingsForm />
    </SettingsPage>
  );
}
