import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { SettingsPage } from "./-components/settings-page";
import { ThrottleSettingsForm } from "./-components/throttle-settings-form";

export const Route = createFileRoute("/admin/settings/throttle")({
  component: ThrottleSettingsRoute,
});

function ThrottleSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_throttle_description()}
      title={m.settings_throttle_title()}
    >
      <ThrottleSettingsForm />
    </SettingsPage>
  );
}
