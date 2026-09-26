import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { Route as rootRoute } from "../__root";
import { AccountSettingsForm } from "./-components/account-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: AccountSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/account",
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
