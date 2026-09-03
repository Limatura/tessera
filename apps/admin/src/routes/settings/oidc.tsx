import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "../__root";
import { OidcSettingsForm } from "./-components/oidc-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: OidcSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/oidc",
});

function OidcSettingsRoute() {
  return (
    <SettingsPage description="透過外部身分提供者登入" title="OIDC 設定">
      <OidcSettingsForm />
    </SettingsPage>
  );
}
