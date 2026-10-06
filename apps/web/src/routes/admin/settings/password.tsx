import { createFileRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { PasswordSettingsForm } from "./-components/password-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/password")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: PasswordSettingsRoute,
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
