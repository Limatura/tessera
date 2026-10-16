import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { ResetSettings } from "./-components/reset-settings";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/reset")({
  component: ResetSettingsRoute,
  staticData: { title: m.nav_settings_reset },
});

function ResetSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_reset_description()}
      title={m.settings_reset_title()}
    >
      <ResetSettings />
    </SettingsPage>
  );
}
