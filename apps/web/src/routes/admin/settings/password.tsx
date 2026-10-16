import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { PasswordSettingsForm } from "./-components/password-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/password")({
  component: PasswordSettingsRoute,
  staticData: { title: m.nav_settings_password },
});

function PasswordSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_password_description()}
      title={m.settings_password_title()}
    >
      <PasswordSettingsForm />
    </SettingsPage>
  );
}
