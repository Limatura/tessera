import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { SettingsCard } from "./-components/settings-card";
import { ShareSettingsForm } from "./-components/share-settings-form";

export const Route = createFileRoute("/admin/settings/share")({
  component: ShareSettingsRoute,
});

function ShareSettingsRoute() {
  return (
    <SettingsCard
      description={m.settings_share_description()}
      title={m.settings_share_title()}
    >
      <ShareSettingsForm />
    </SettingsCard>
  );
}
