import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { SettingsPage } from "./-components/settings-page";
import { StorageBackendsCard } from "./-components/storage-backends-card";
import { StorageSettingsForm } from "./-components/storage-settings-form";

export const Route = createFileRoute("/admin/settings/storage")({
  component: StorageSettingsRoute,
  staticData: { title: m.nav_settings_storage },
});

/**
 * Two independent parts: the backend list uses the `/admin/storage/backends`
 * endpoints, the rest `PUT /admin/settings`. In one form, an error on one side
 * would block the other.
 */
function StorageSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_storage_description()}
      title={m.settings_storage_title()}
    >
      <StorageBackendsCard />
      <StorageSettingsForm />
    </SettingsPage>
  );
}
