import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { SettingsCard } from "./-components/settings-card";
import { ThrottleSettingsForm } from "./-components/throttle-settings-form";

export const Route = createFileRoute("/admin/settings/throttle")({
  component: ThrottleSettingsRoute,
});

function ThrottleSettingsRoute() {
  return (
    <SettingsCard
      description={m.settings_throttle_description()}
      title={m.settings_throttle_title()}
    >
      <ThrottleSettingsForm />
    </SettingsCard>
  );
}
