import { createFileRoute } from "@tanstack/react-router";

import { SpeedTestRunner } from "@/components/speed-test/speed-test-runner";
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
      <SpeedTestRunner />
    </div>
  );
}
