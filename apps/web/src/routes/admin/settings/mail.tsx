import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { MailSettingsForm } from "./-components/mail-settings-form";
import { SettingsCard } from "./-components/settings-card";

export const Route = createFileRoute("/admin/settings/mail")({
  component: MailSettingsRoute,
});

function MailSettingsRoute() {
  return (
    <SettingsCard
      description={m.settings_mail_description()}
      title={m.settings_mail_title()}
    >
      <MailSettingsForm />
    </SettingsCard>
  );
}
