import { createFileRoute } from "@tanstack/react-router";

import { GlassBackground } from "@/components/glass-background";

export const Route = createFileRoute("/s/")({
  component: RouteComponent,
});

function RouteComponent() {
  return (
    <>
      <GlassBackground />
      <div className="" />
    </>
  );
}
