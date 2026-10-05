import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { GlassBackground } from "@/components/glass-background";

import { RegisterForm } from "./-components/register-form";

/** 邀請制的站台把碼放在連結裡,填好給人點,不必再手動抄一次。 */
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
