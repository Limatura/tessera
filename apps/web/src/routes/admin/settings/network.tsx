import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { NetworkSettingsForm } from "./-components/network-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/network")({
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
