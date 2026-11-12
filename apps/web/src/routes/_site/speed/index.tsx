import {
  IconArrowDown,
  IconArrowUp,
  IconGauge,
  IconPlayerPlay,
  IconPlayerStop,
} from "@tabler/icons-react";
import { createFileRoute } from "@tanstack/react-router";
import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { formatBytes } from "@tessera/ui/lib/format";
import { cn } from "@tessera/ui/lib/utils";
import { useEffect, useRef, useState } from "react";

import { GlassBackground } from "@/components/glass-background";
import { useCapabilities } from "@/hooks/use-capabilities";
import { apiErrorMessage } from "@/lib/api-error";
import {
  SpeedTestError,
  measureDownload,
  measureLatency,
  measureUpload,
} from "@/lib/speed-test";
import type { TransferProgress } from "@/lib/speed-test";
import * as m from "@/paraglide/messages";

export const Route = createFileRoute("/_site/speed/")({
  component: SpeedTestPage,
});

type Phase = "idle" | "latency" | "download" | "upload" | "done";

interface Results {
  latencyMs?: number;
  jitterMs?: number;
  downloadMbps?: number;
  uploadMbps?: number;
  bytes?: number;
}

function SpeedTestPage() {
  const { data: capabilities, isPending } = useCapabilities();
  const available = capabilities?.speed_test === true;

  return (
    <div className="flex min-h-svh w-full items-center justify-center p-6 pt-20 md:p-10">
      <GlassBackground />
      <div className="w-full max-w-lg">
        <Card className="border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 font-medium text-xl tracking-tight">
              <IconGauge size={20} />
              {m.speed_title()}
            </CardTitle>
            <CardDescription>
              {available || isPending
                ? m.speed_description()
                : m.speed_unavailable()}
            </CardDescription>
          </CardHeader>
          {available && (
            <CardContent>
              <SpeedTestRunner />
            </CardContent>
          )}
        </Card>
      </div>
    </div>
  );
}

function SpeedTestRunner() {
  const [phase, setPhase] = useState<Phase>("idle");
  const [results, setResults] = useState<Results>({});
  const [live, setLive] = useState<TransferProgress | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const controller = useRef<AbortController | null>(null);

  // Leaving the page stops the transfer instead of letting it run unseen.
  useEffect(() => () => controller.current?.abort(), []);

  const running =
    phase === "latency" || phase === "download" || phase === "upload";

  const handleStart = async () => {
    const abort = new AbortController();
    controller.current = abort;
    setResults({});
    setFailure(null);
    setLive(null);
    try {
      setPhase("latency");
      const latency = await measureLatency(abort.signal);
      setResults({ ...latency });

      setPhase("download");
      const download = await measureDownload(abort.signal, setLive);
      setResults((prev) => ({
        ...prev,
        bytes: download.bytes,
        downloadMbps: download.mbps,
      }));
      setLive(null);

      // Upload the same amount the server sent: that is its per-test maximum.
      setPhase("upload");
      const upload = await measureUpload(download.bytes, abort.signal, setLive);
      setResults((prev) => ({ ...prev, uploadMbps: upload.mbps }));
      setPhase("done");
    } catch (error) {
      if (abort.signal.aborted) {
        setPhase("idle");
        return;
      }
      setFailure(
        error instanceof SpeedTestError
          ? apiErrorMessage(error.body)
          : m.speed_network_error()
      );
      setPhase("idle");
    } finally {
      setLive(null);
    }
  };

  const handleStop = () => controller.current?.abort();

  const liveMbps = (direction: Phase) =>
    phase === direction && live ? live.mbps : undefined;

  return (
    <div className="flex flex-col gap-6">
      <div className="grid grid-cols-2 gap-3">
        <Metric
          active={phase === "download"}
          icon={<IconArrowDown className="size-4" />}
          label={m.speed_download()}
          unit="Mbps"
          value={results.downloadMbps ?? liveMbps("download")}
        />
        <Metric
          active={phase === "upload"}
          icon={<IconArrowUp className="size-4" />}
          label={m.speed_upload()}
          unit="Mbps"
          value={results.uploadMbps ?? liveMbps("upload")}
        />
        <Metric
          active={phase === "latency"}
          label={m.speed_latency()}
          unit="ms"
          value={results.latencyMs}
        />
        <Metric
          active={phase === "latency"}
          label={m.speed_jitter()}
          unit="ms"
          value={results.jitterMs}
        />
      </div>

      <p aria-live="polite" className="min-h-5 text-muted-foreground text-sm">
        {statusText(phase, live, results)}
      </p>

      {failure && (
        <p className="text-destructive text-sm" role="alert">
          {failure}
        </p>
      )}

      {running ? (
        <Button onClick={handleStop} type="button" variant="outline">
          <IconPlayerStop />
          {m.speed_stop()}
        </Button>
      ) : (
        <Button onClick={handleStart} type="button">
          <IconPlayerPlay />
          {phase === "done" ? m.speed_run_again() : m.speed_start()}
        </Button>
      )}

      <p className="text-muted-foreground text-xs">{m.speed_note()}</p>
    </div>
  );
}

function statusText(
  phase: Phase,
  live: TransferProgress | null,
  results: Results
): string {
  switch (phase) {
    case "latency": {
      return m.speed_status_latency();
    }
    case "download":
    case "upload": {
      const moved = formatBytes(live?.bytes ?? 0);
      return phase === "download"
        ? m.speed_status_download({ bytes: moved })
        : m.speed_status_upload({ bytes: moved });
    }
    case "done": {
      return m.speed_status_done({ bytes: formatBytes(results.bytes ?? 0) });
    }
    default: {
      return "";
    }
  }
}

function Metric({
  label,
  value,
  unit,
  icon,
  active,
}: {
  label: string;
  value: number | undefined;
  unit: string;
  icon?: React.ReactNode;
  active: boolean;
}) {
  return (
    <div
      className={cn(
        "rounded-lg border bg-background/40 p-3 transition-colors",
        active && "border-primary"
      )}
    >
      <div className="flex items-center gap-1 text-muted-foreground text-xs">
        {icon}
        {label}
      </div>
      <div className="mt-1 font-medium text-2xl tabular-nums">
        {value === undefined
          ? m.common_dash_placeholder()
          : formatMetric(value)}
        <span className="ml-1 font-normal text-muted-foreground text-sm">
          {unit}
        </span>
      </div>
    </div>
  );
}

function formatMetric(value: number): string {
  const digits = value >= 100 ? 0 : 1;
  return value.toFixed(digits);
}
