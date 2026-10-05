import { createFileRoute } from "@tanstack/react-router";

import { GlassBackground } from "@/components/glass-background";
import { ShareView } from "@/routes/_site/s/$code/-components/share-view";

export const Route = createFileRoute("/_site/s/$code/")({
  component: RouteComponent,
});

function RouteComponent() {
  const { code } = Route.useParams();

  return (
    <>
      <GlassBackground />
      <div className="flex min-h-screen flex-col items-center justify-center gap-4 p-4">
        <ShareView code={code} />
      </div>
    </>
  );
}
