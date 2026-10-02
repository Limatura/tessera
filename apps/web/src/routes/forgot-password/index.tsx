import { createFileRoute } from "@tanstack/react-router";

import { GlassBackground } from "@/components/glass-background";

import { ForgotPasswordForm } from "./-components/forgot-password-form";

export const Route = createFileRoute("/forgot-password/")({
  component: RouteComponent,
});

function RouteComponent() {
  return (
    <div className="flex min-h-svh w-full items-center justify-center p-6 md:p-10">
      <GlassBackground />
      <div className="w-full max-w-sm">
        <ForgotPasswordForm />
      </div>
    </div>
  );
}
