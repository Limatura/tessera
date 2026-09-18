import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "../__root";
import { SettingsPage } from "./-components/settings-page";
import { ShareSettingsForm } from "./-components/share-settings-form";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: ShareSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/share",
});

function ShareSettingsRoute() {
  return (
    <SettingsPage
      description="取件碼、下載授權時效、清掃,以及取件紀錄"
      title="分享與取件"
    >
      <ShareSettingsForm />
    </SettingsPage>
  );
}
