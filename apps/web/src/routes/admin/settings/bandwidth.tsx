import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { BandwidthSettingsForm } from "./-components/bandwidth-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/bandwidth")({
  component: BandwidthSettingsRoute,
  staticData: { title: m.nav_settings_bandwidth },
});

function BandwidthSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_bandwidth_description()}
      title={m.settings_bandwidth_title()}
    >
      <BandwidthSettingsForm />
    </SettingsPage>
  );
}
