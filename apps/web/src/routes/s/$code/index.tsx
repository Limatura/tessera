import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/s/$code/")({
  component: RouteComponent,
});

function RouteComponent() {
  return <div>Hello "/s/$code/"!</div>;
}
