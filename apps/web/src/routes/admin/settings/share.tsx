import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { SettingsPage } from "./-components/settings-page";
import { ShareSettingsForm } from "./-components/share-settings-form";

export const Route = createFileRoute("/admin/settings/share")({
  component: ShareSettingsRoute,
});

function ShareSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_share_description()}
      title={m.settings_share_title()}
    >
      <ShareSettingsForm />
    </SettingsPage>
  );
}
