import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "../__root";
import { SettingsPage } from "./-components/settings-page";
import { UploadSettingsForm } from "./-components/upload-settings-form";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: UploadSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/upload",
});

function UploadSettingsRoute() {
  return (
    <SettingsPage description="上傳限流、大小上限與保存時間" title="上傳設置">
      <UploadSettingsForm />
    </SettingsPage>
  );
}
