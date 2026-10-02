import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { GlassBackground } from "@/components/glass-background";

import { RegisterConfirmForm } from "./-components/register-confirm-form";

const searchSchema = z.object({
  token: z.string().optional(),
});

export const Route = createFileRoute("/register/confirm/")({
  component: RouteComponent,
  validateSearch: searchSchema,
});

function RouteComponent() {
  const { token } = Route.useSearch();
  return (
    <div className="flex min-h-svh w-full items-center justify-center p-6 md:p-10">
      <GlassBackground />
      <div className="w-full max-w-sm">
        <RegisterConfirmForm token={token ?? null} />
      </div>
    </div>
  );
}
