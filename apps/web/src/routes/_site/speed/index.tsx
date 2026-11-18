import { IconGauge } from "@tabler/icons-react";
import { createFileRoute } from "@tanstack/react-router";

import { GlassBackground } from "@/components/glass-background";
import { SpeedTestRunner } from "@/components/speed-test/speed-test-runner";
import { useCapabilities } from "@/hooks/use-capabilities";
import * as m from "@/paraglide/messages";

export const Route = createFileRoute("/_site/speed/")({
  component: SpeedTestPage,
});

function SpeedTestPage() {
  const { data: capabilities, isPending } = useCapabilities();
  const available = capabilities?.speed_test === true;

  return (
    <div className="mx-auto w-full max-w-2xl px-6 pt-24 pb-16 md:px-10">
      <GlassBackground />
      <header className="mb-8 flex flex-col items-center gap-1 text-center">
        <h1 className="flex items-center gap-2 font-semibold text-2xl tracking-tight">
          <IconGauge className="text-primary" size={24} />
          {m.speed_title()}
        </h1>
        <p className="text-muted-foreground text-sm">
          {available || isPending
            ? m.speed_description()
            : m.speed_unavailable()}
        </p>
      </header>
      {available && <SpeedTestRunner />}
    </div>
  );
}
