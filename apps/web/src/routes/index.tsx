import { createRoute } from "@tanstack/react-router";

import { UploadPanel } from "./-components/upload-panel";
import { Route as rootRoute } from "./__root";

export const Route = createRoute({
  component: HomeComponent,
  getParentRoute: () => rootRoute,
  path: "/",
});

function HomeComponent() {
  return <UploadPanel />;
}
