import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

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
      description="誰能註冊、新帳號拿到什麼角色，以及登入會話與紀錄的保留時間"
      title="帳號與註冊"
    >
      <AccountSettingsForm />
    </SettingsPage>
  );
}
