import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { SettingsPage } from "./-components/settings-page";
import { SiteSettingsForm } from "./-components/site-settings-form";

export const Route = createFileRoute("/admin/settings/site")({
  component: SiteSettingsRoute,
});

function SiteSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_site_description()}
      title={m.settings_site_title()}
    >
      <SiteSettingsForm />
    </SettingsPage>
  );
}
