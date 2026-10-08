import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { OgCardSettingsForm } from "./-components/og-card-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/og")({
  component: OgCardSettingsRoute,
});

function OgCardSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_og_description()}
      title={m.settings_og_title()}
    >
      <OgCardSettingsForm />
    </SettingsPage>
  );
}
