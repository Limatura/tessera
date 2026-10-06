import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { GlassBackground } from "@/components/glass-background";

import { VerifyEmailCard } from "./-components/verify-email-card";

/** 驗證信裡的連結開到這裡(見 apps/server/src/api/verification.rs)。不需要登入。 */
const searchSchema = z.object({
  token: z.string().optional(),
});

export const Route = createFileRoute("/_site/verify-email/")({
  component: RouteComponent,
  validateSearch: searchSchema,
});

function RouteComponent() {
  const { token } = Route.useSearch();

  return (
    <div className="flex min-h-svh w-full items-center justify-center p-6 md:p-10">
      <GlassBackground />
      <div className="w-full max-w-sm">
        <VerifyEmailCard token={token} />
      </div>
    </div>
  );
}
