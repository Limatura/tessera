import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { LogsSettingsForm } from "./-components/logs-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/logs")({
  component: LogsSettingsRoute,
  staticData: { title: m.nav_settings_logs },
});

function LogsSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_logs_description()}
      title={m.settings_logs_title()}
    >
      <LogsSettingsForm />
    </SettingsPage>
  );
}
