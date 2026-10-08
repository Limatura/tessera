import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { FeatureSettingsForm } from "./-components/feature-settings-form";
import { SettingsCard } from "./-components/settings-card";

export const Route = createFileRoute("/admin/settings/features")({
  component: FeatureSettingsRoute,
});

function FeatureSettingsRoute() {
  return (
    <SettingsCard
      description={m.settings_features_description()}
      title={m.settings_features_title()}
    >
      <FeatureSettingsForm />
    </SettingsCard>
  );
}
