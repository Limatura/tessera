import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

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
    <SettingsPage description="儲存後端位址與分片上傳大小" title="存儲配置">
      <StorageSettingsForm />
    </SettingsPage>
  );
}
