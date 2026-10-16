import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { GlassBackground } from "@/components/glass-background";

import { LoginForm } from "./-components/login-form";

/**
 * Pages that need a sign-in (the admin panel included) send users here with the address
 * they wanted.
 */
const searchSchema = z.object({
  /**
   * An invite code is carried through to `start`; the server spends it only when the
   * account is created.
   */
  invite: z.string().optional(),
  /**
   * The code the server sends back when a provider sign-in fails; the parameter name is
   * the same for every provider.
   */
  oidc_error: z.string().optional(),
  redirect: z.string().optional(),
});

export const Route = createFileRoute("/_site/login/")({
  component: RouteComponent,
  validateSearch: searchSchema,
});

function RouteComponent() {
  return (
    <div className="flex min-h-svh w-full items-center justify-center p-6 md:p-10">
      <GlassBackground />
      <div className="w-full max-w-sm">
        <LoginForm />
      </div>
    </div>
  );
}
