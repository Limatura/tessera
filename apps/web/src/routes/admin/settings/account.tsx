import { createFileRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { AccountSettingsForm } from "./-components/account-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/account")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
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
