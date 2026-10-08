import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { AccountSettingsForm } from "./-components/account-settings-form";
import { SettingsCard } from "./-components/settings-card";

export const Route = createFileRoute("/admin/settings/account")({
  component: AccountSettingsRoute,
});

function AccountSettingsRoute() {
  return (
    <SettingsCard
      description={m.settings_account_description()}
      title={m.settings_account_title()}
    >
      <AccountSettingsForm />
    </SettingsCard>
  );
}
