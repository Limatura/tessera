import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { GlassBackground } from "@/components/glass-background";
import { requireAuth } from "@/lib/route-guard";

import { DeviceCard } from "./-components/device-card";

/**
 * Where `tess login` sends the person, with `?code=` filled in, to approve
 * the terminal it runs in (see apps/server/src/api/device.rs). Signing in comes
 * first, and the sign-in page brings them back here with the code intact.
 */
const searchSchema = z.object({
  code: z.string().optional(),
});

export const Route = createFileRoute("/_site/device/")({
  beforeLoad: ({ context, location }) =>
    requireAuth(context.queryClient, location.href),
  component: RouteComponent,
  validateSearch: searchSchema,
});

function RouteComponent() {
  const { code } = Route.useSearch();

  return (
    <>
      <GlassBackground />
      <div className="flex min-h-screen flex-col items-center justify-center gap-4 p-4">
        <DeviceCard code={code} />
      </div>
    </>
  );
}
