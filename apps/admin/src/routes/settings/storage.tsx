import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { Route as rootRoute } from "../__root";
import { SettingsPage } from "./-components/settings-page";
import { StorageSettingsForm } from "./-components/storage-settings-form";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: StorageSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/storage",
});

function StorageSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_storage_description()}
      title={m.settings_storage_title()}
    >
      <StorageSettingsForm />
    </SettingsPage>
  );
}
