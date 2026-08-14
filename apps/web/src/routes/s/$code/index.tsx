import { createFileRoute } from "@tanstack/react-router";

import { GlassBackground } from "@/components/glass-background";

export const Route = createFileRoute("/s/$code/")({
  component: RouteComponent,
});

function RouteComponent() {
  return (
    <>
      <GlassBackground />

      <div className="flex min-h-screen flex-col items-center justify-center gap-4">
        <h1 className="text-4xl font-bold">This is a secret page</h1>
        <p className="text-lg text-muted-foreground">
          You have accessed a secret route with a code parameter.
        </p>
        <p className="text-sm text-muted-foreground">The code is: $code</p>
      </div>
    </>
  );
}
