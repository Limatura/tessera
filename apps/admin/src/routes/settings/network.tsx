import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "../__root";
import { NetworkSettingsForm } from "./-components/network-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: NetworkSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/network",
});

function NetworkSettingsRoute() {
  return (
    <SettingsPage
      description="對外網址、可信任的反向代理,以及 session cookie 與逾時"
      title="網路與代理"
    >
      <NetworkSettingsForm />
    </SettingsPage>
  );
}
