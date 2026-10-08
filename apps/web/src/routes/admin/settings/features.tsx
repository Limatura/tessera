import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { FeatureSettingsForm } from "./-components/feature-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/features")({
  component: FeatureSettingsRoute,
});

function FeatureSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_features_description()}
      title={m.settings_features_title()}
    >
      <FeatureSettingsForm />
    </SettingsPage>
  );
}
