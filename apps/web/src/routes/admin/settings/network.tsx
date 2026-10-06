import { createFileRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { NetworkSettingsForm } from "./-components/network-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/network")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: NetworkSettingsRoute,
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
