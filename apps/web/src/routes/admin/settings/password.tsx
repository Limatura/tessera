import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { PasswordSettingsForm } from "./-components/password-settings-form";
import { SettingsCard } from "./-components/settings-card";

export const Route = createFileRoute("/admin/settings/password")({
  component: PasswordSettingsRoute,
});

function PasswordSettingsRoute() {
  return (
    <SettingsCard
      description={m.settings_password_description()}
      title={m.settings_password_title()}
    >
      <PasswordSettingsForm />
    </SettingsCard>
  );
}
