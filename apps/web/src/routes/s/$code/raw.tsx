import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/s/$code/raw")({
  component: RouteComponent,
});

function RouteComponent() {
  return <div>Hello "/s/$code/raw"!</div>;
}
