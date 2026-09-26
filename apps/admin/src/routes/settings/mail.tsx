import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { Route as rootRoute } from "../__root";
import { MailSettingsForm } from "./-components/mail-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: MailSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/mail",
});

function MailSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_mail_description()}
      title={m.settings_mail_title()}
    >
      <MailSettingsForm />
    </SettingsPage>
  );
}
