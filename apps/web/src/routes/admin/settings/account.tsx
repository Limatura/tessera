import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { AccountSettingsForm } from "./-components/account-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/account")({
  component: AccountSettingsRoute,
});

function AccountSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_account_description()}
      title={m.settings_account_title()}
    >
      <AccountSettingsForm />
    </SettingsPage>
  );
}
