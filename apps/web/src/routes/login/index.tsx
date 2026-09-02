import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { GlassBackground } from "../../components/glass-background";
import { LoginForm } from "./-components/login-form";

/** apps/admin 未登入時會把人送來這裡，並帶上原本要去的位址。 */
const searchSchema = z.object({
  redirect: z.string().optional(),
});

export const Route = createFileRoute("/login/")({
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
