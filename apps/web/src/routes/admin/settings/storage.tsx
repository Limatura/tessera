import { createFileRoute } from "@tanstack/react-router";

import { AdminShell } from "@/components/admin/admin-shell";
import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { StorageBackendsCard } from "./-components/storage-backends-card";
import { StorageSettingsForm } from "./-components/storage-settings-form";

export const Route = createFileRoute("/admin/settings/storage")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: StorageSettingsRoute,
});

/**
 * 兩張各自獨立的卡：分片上傳大小走 `PUT /admin/settings`，backend 列表走
 * `/admin/storage/backends` 系列端點——兩邊互不相干，合成一份表單只會讓其中一邊
 * 的錯誤擋住另一邊。
 */
function StorageSettingsRoute() {
  return (
    <AdminShell title={m.settings_page_shell_title()}>
      <StorageSettingsForm />
      <StorageBackendsCard />
    </AdminShell>
  );
}
