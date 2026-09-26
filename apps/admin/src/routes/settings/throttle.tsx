import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { Route as rootRoute } from "../__root";
import { SettingsPage } from "./-components/settings-page";
import { ThrottleSettingsForm } from "./-components/throttle-settings-form";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: ThrottleSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/throttle",
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
