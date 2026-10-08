import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { NetworkSettingsForm } from "./-components/network-settings-form";
import { SettingsCard } from "./-components/settings-card";

export const Route = createFileRoute("/admin/settings/network")({
  component: NetworkSettingsRoute,
});

function NetworkSettingsRoute() {
  return (
    <SettingsCard
      description={m.settings_network_description()}
      title={m.settings_network_title()}
    >
      <NetworkSettingsForm />
    </SettingsCard>
  );
}
