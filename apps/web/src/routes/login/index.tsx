import { createFileRoute } from "@tanstack/react-router";

import { GlassBackground } from "../../components/glass-background";
import { LoginForm } from "./-components/login-form";

export const Route = createFileRoute("/login/")({
  component: RouteComponent,
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
