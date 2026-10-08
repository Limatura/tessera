import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { SettingsPage } from "./-components/settings-page";
import { StorageBackendsCard } from "./-components/storage-backends-card";
import { StorageSettingsForm } from "./-components/storage-settings-form";

export const Route = createFileRoute("/admin/settings/storage")({
  component: StorageSettingsRoute,
});

/**
 * 兩塊各自獨立：backend 列表走 `/admin/storage/backends` 系列端點，其餘走
 * `PUT /admin/settings` —— 兩邊互不相干，合成一份表單只會讓其中一邊的錯誤擋住另一邊。
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
