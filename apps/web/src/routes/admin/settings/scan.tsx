import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { ScanSettingsForm } from "./-components/scan-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/scan")({
  component: ScanSettingsRoute,
  staticData: { title: m.nav_settings_scan },
});

function ScanSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_scan_description()}
      title={m.settings_scan_title()}
    >
      <ScanSettingsForm />
    </SettingsPage>
  );
}
