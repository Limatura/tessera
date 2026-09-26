import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

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
      description={m.settings_network_description()}
      title={m.settings_network_title()}
    >
      <NetworkSettingsForm />
    </SettingsPage>
  );
}
