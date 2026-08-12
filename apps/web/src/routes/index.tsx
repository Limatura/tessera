import { createRoute } from "@tanstack/react-router";

import { UploadFlow } from "./-components/upload-flow";
import { Route as rootRoute } from "./__root";

export const Route = createRoute({
  component: HomeComponent,
  getParentRoute: () => rootRoute,
  path: "/",
});

function HomeComponent() {
  return <UploadFlow />;
}
