import { createFileRoute } from "@tanstack/react-router";

import { SpeedTestRunner } from "@/components/speed-test/speed-test-runner";
import { readSetting, useSettings } from "@/hooks/admin/use-settings";
import { useCapabilities } from "@/hooks/use-capabilities";
import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

export const Route = createFileRoute("/admin/speed/")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: AdminSpeedComponent,
  staticData: { title: m.nav_speed_test },
});

/** The visitors' test, run from the panel so the operator sees what they see. */
function AdminSpeedComponent() {
  const { data: capabilities, isPending } = useCapabilities();
  const { data: settings } = useSettings();

  if (isPending) {
    return null;
  }
  if (capabilities?.speed_test !== true) {
    return (
      <p className="text-muted-foreground text-sm">{m.speed_unavailable()}</p>
    );
  }
  return (
    <div className="mx-auto w-full max-w-xl">
      <SpeedTestRunner remember={!isBandwidthLimited(settings)} />
    </div>
  );
}

const BANDWIDTH_KEYS = [
  "bandwidth.upload",
  "bandwidth.download",
  "bandwidth.upload_per_transfer",
  "bandwidth.download_per_transfer",
];

/** Whether any limit is set, so a test would be measuring the limit. */
function isBandwidthLimited(settings: Parameters<typeof readSetting>[0]) {
  return BANDWIDTH_KEYS.some((key) => {
    const value = readSetting(settings, key);
    return typeof value === "number" && value > 0;
  });
}
