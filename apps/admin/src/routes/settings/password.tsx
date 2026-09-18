import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "../__root";
import { PasswordSettingsForm } from "./-components/password-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: PasswordSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/password",
});

function PasswordSettingsRoute() {
  return (
    <SettingsPage
      description="密碼長度規則與 Argon2id 的成本參數"
      title="密碼與雜湊"
    >
      <PasswordSettingsForm />
    </SettingsPage>
  );
}
