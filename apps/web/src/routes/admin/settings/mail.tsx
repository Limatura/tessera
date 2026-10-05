import { createFileRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { MailSettingsForm } from "./-components/mail-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/mail")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: MailSettingsRoute,
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
