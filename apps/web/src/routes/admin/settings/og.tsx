import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { OgCardSettingsForm } from "./-components/og-card-settings-form";
import { SettingsCard } from "./-components/settings-card";

export const Route = createFileRoute("/admin/settings/og")({
  component: OgCardSettingsRoute,
});

function OgCardSettingsRoute() {
  return (
    <SettingsCard
      description={m.settings_og_description()}
      title={m.settings_og_title()}
    >
      <OgCardSettingsForm />
    </SettingsCard>
  );
}
