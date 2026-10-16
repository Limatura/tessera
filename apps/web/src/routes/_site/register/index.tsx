import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { GlassBackground } from "@/components/glass-background";

import { RegisterForm } from "./-components/register-form";

/** Invite-only instances put the code in the link and prefill it, so it is never copied by hand. */
const searchSchema = z.object({
  invite: z.string().optional(),
});

export const Route = createFileRoute("/_site/register/")({
  component: RouteComponent,
  validateSearch: searchSchema,
});

function RouteComponent() {
  return (
    <div className="flex min-h-svh w-full items-center justify-center p-6 md:p-10">
      <GlassBackground />
      <div className="w-full max-w-sm">
        <RegisterForm />
      </div>
    </div>
  );
}
