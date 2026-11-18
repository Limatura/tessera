import {
  IconArrowDown,
  IconArrowUp,
  IconPlayerStop,
  IconRefresh,
} from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { formatBytes } from "@tessera/ui/lib/format";
import { cn } from "@tessera/ui/lib/utils";
import { useEffect, useRef, useState } from "react";

import { apiErrorMessage } from "@/lib/api-error";
import {
  SpeedTestError,
  measureDownload,
  measureLatency,
  measureUpload,
  RAMP_UP_MS,
  windowedMbps,
} from "@/lib/speed-test";
import type { RateSample, TransferProgress } from "@/lib/speed-test";
import * as m from "@/paraglide/messages";

import { SpeedGauge } from "./speed-gauge";
import { ThroughputChart } from "./throughput-chart";
import type { ThroughputPoint } from "./throughput-chart";

type Phase = "idle" | "latency" | "download" | "upload" | "done";
type Direction = "download" | "upload";

interface Traces {
  download: ThroughputPoint[];
  upload: ThroughputPoint[];
}

const NO_TRACES: Traces = { download: [], upload: [] };

interface Results {
  latencyMs?: number;
  jitterMs?: number;
  downloadMbps?: number;
  uploadMbps?: number;
  bytes?: number;
}

/** How often the dial and chart take a reading. */
const TICK_MS = 100;
/** The rate is taken over this much recent time, then eased, so a steady line reads steady. */
const RATE_WINDOW_MS = 1000;
const SMOOTHING = 0.15;
/** How long the dial rests on a direction's result before moving on. */
const SETTLE_MS = 1200;

/** Runs one speed test and shows it live; the site page and the admin page both host it. */
export function SpeedTestRunner() {
  const [phase, setPhase] = useState<Phase>("idle");
  const [results, setResults] = useState<Results>({});
  const [live, setLive] = useState<TransferProgress | null>(null);
  const [liveMbps, setLiveMbps] = useState(0);
  const [traces, setTraces] = useState<Traces>(NO_TRACES);
  const [failure, setFailure] = useState<string | null>(null);
  const controller = useRef<AbortController | null>(null);
  const samples = useRef<RateSample[]>([]);
  const latestBytes = useRef(0);
  const latestRate = useRef(0);
  // How much this direction moves, when known; 0 while the stream decides.
  const expectedBytes = useRef(0);
  const smoothed = useRef(0);
  // Set while the dial rests on a result, so the clock leaves it alone.
  const settling = useRef(false);

  // Leaving the page stops the transfer instead of letting it run unseen.
  useEffect(() => () => controller.current?.abort(), []);

  const running =
    phase === "latency" || phase === "download" || phase === "upload";

  const resetLive = () => {
    samples.current = [];
    latestBytes.current = 0;
    latestRate.current = 0;
    expectedBytes.current = 0;
    smoothed.current = 0;
    setLive(null);
    setLiveMbps(0);
  };

  // Progress events arrive in bursts (a socket buffer drains, then nothing for
  // a while), so the dial and chart are fed by a steady clock instead: each
  // tick reads the bytes moved so far, which keeps them moving between events.
  const trackProgress = (progress: TransferProgress) => {
    latestBytes.current = progress.bytes;
    latestRate.current = progress.mbps;
    setLive(progress);
  };

  const direction: Direction | null =
    phase === "download" || phase === "upload" ? phase : null;
  useEffect(() => {
    if (!direction) {
      return;
    }
    const started = performance.now();
    samples.current = [{ at: started, bytes: 0 }];
    smoothed.current = 0;
    let primed = false;
    const timer = setInterval(() => {
      const now = performance.now();
      // Once everything has been sent, only the reply is pending. Reading the
      // rate then would watch it fall toward zero, so hold the last reading.
      const finished =
        expectedBytes.current > 0 &&
        latestBytes.current >= expectedBytes.current;
      if (finished || settling.current) {
        return;
      }
      // Until the warm-up is over and some bytes have been counted, each tick
      // only moves the baseline. An upload counts its first part all at once
      // when the reply comes, and that step would read as a burst.
      if (now - started < RAMP_UP_MS || !primed) {
        samples.current = [{ at: now, bytes: latestBytes.current }];
        primed = latestBytes.current > 0;
        return;
      }
      samples.current.push({ at: now, bytes: latestBytes.current });
      // A download is read from its bytes; an upload from its parts' rates,
      // the same ones its result is made of.
      const rate =
        direction === "upload"
          ? latestRate.current
          : windowedMbps(samples.current, RATE_WINDOW_MS);
      smoothed.current += (rate - smoothed.current) * SMOOTHING;
      setLiveMbps(smoothed.current);
      const point = { mbps: smoothed.current, t: (now - started) / 1000 };
      setTraces((prev) => ({
        ...prev,
        [direction]: [...prev[direction], point],
      }));
    }, TICK_MS);
    return () => clearInterval(timer);
  }, [direction]);

  // The dial ends where the card does: it eases onto the result and rests
  // there a moment, instead of the next direction cutting it off mid-reading.
  const settleOn = async (mbps: number, signal: AbortSignal) => {
    settling.current = true;
    setLiveMbps(mbps);
    // oxlint-disable-next-line promise/avoid-new -- a timer that an abort cuts short
    await new Promise<void>((resolve) => {
      const timer = setTimeout(resolve, SETTLE_MS);
      signal.addEventListener(
        "abort",
        () => {
          clearTimeout(timer);
          resolve();
        },
        { once: true }
      );
    });
    settling.current = false;
    signal.throwIfAborted();
  };

  const handleStart = async () => {
    const abort = new AbortController();
    controller.current = abort;
    setResults({});
    setFailure(null);
    setTraces(NO_TRACES);
    resetLive();
    try {
      setPhase("latency");
      const latency = await measureLatency(abort.signal);
      setResults({ ...latency });

      setPhase("download");
      const download = await measureDownload(abort.signal, trackProgress);
      setResults((prev) => ({
        ...prev,
        bytes: download.bytes,
        downloadMbps: download.mbps,
      }));
      await settleOn(download.mbps, abort.signal);
      resetLive();

      // Upload the same amount the server sent: that is its per-test maximum.
      expectedBytes.current = download.bytes;
      setPhase("upload");
      const upload = await measureUpload(
        download.bytes,
        latency.latencyMs,
        abort.signal,
        trackProgress
      );
      setResults((prev) => ({ ...prev, uploadMbps: upload.mbps }));
      await settleOn(upload.mbps, abort.signal);
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
      resetLive();
    }
  };

  const handleStop = () => controller.current?.abort();

  // The dial reads only while bytes move; before and after it is the button.
  const dialMbps = direction ? liveMbps : 0;
  const latencyRunning = phase === "latency";

  return (
    <div className="flex flex-col items-center gap-8">
      <section className="w-full overflow-hidden rounded-3xl border border-border/60 bg-card/60 shadow-black/5 shadow-xl backdrop-blur-2xl">
        <div className="grid grid-cols-2 divide-x divide-border/60">
          <SpeedResult
            active={phase === "download"}
            icon={<IconArrowDown className="size-4" />}
            label={m.speed_download()}
            live={phase === "download" ? liveMbps : undefined}
            points={traces.download}
            value={results.downloadMbps}
          />
          <SpeedResult
            active={phase === "upload"}
            icon={<IconArrowUp className="size-4" />}
            label={m.speed_upload()}
            live={phase === "upload" ? liveMbps : undefined}
            points={traces.upload}
            value={results.uploadMbps}
          />
        </div>
        <dl className="flex items-center justify-center gap-8 border-border/60 border-t px-4 py-3 text-sm">
          <SmallResult
            active={latencyRunning}
            label={m.speed_latency()}
            value={results.latencyMs}
          />
          <SmallResult
            active={latencyRunning}
            label={m.speed_jitter()}
            value={results.jitterMs}
          />
        </dl>
      </section>

      <SpeedGauge
        center={
          !running && (
            <button
              aria-label={
                phase === "done" ? m.speed_run_again() : m.speed_start()
              }
              className="relative flex size-28 items-center justify-center rounded-full bg-primary font-semibold text-primary-foreground text-xl tracking-wide shadow-lg shadow-primary/30 outline-none transition-transform hover:scale-105 focus-visible:ring-4 focus-visible:ring-primary/40 active:scale-95"
              onClick={handleStart}
              type="button"
            >
              <span
                aria-hidden="true"
                className="absolute inset-0 animate-ping rounded-full bg-primary/25 [animation-duration:2.4s]"
              />
              <span className="relative">
                {phase === "done" ? (
                  <IconRefresh className="size-9" />
                ) : (
                  m.speed_go()
                )}
              </span>
            </button>
          )
        }
        className="max-w-sm"
        idle={!running}
        mbps={dialMbps}
      >
        {running && (
          <>
            <span className="font-semibold text-5xl tabular-nums tracking-tight">
              {direction ? formatMetric(dialMbps) : m.common_dash_placeholder()}
            </span>
            <span className="text-muted-foreground text-sm">Mbps</span>
          </>
        )}
      </SpeedGauge>

      <div className="-mt-4 flex min-h-9 flex-col items-center gap-3">
        <p aria-live="polite" className="text-muted-foreground text-sm">
          {statusText(phase, live, results)}
        </p>
        {failure && (
          <p className="text-destructive text-sm" role="alert">
            {failure}
          </p>
        )}
        {running && (
          <Button onClick={handleStop} size="sm" type="button" variant="ghost">
            <IconPlayerStop />
            {m.speed_stop()}
          </Button>
        )}
      </div>

      <p className="max-w-sm text-center text-muted-foreground text-xs">
        {m.speed_note()}
      </p>
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

/**
 * One direction in the results strip: its number, and the trace of how the
 * rate moved. While it runs it shows the live reading in the accent colour;
 * once done, the measured result.
 */
function SpeedResult({
  label,
  icon,
  active,
  live,
  value,
  points,
}: {
  label: string;
  icon: React.ReactNode;
  active: boolean;
  live: number | undefined;
  value: number | undefined;
  points: ThroughputPoint[];
}) {
  const shown = value ?? live;
  return (
    <div className="flex flex-col gap-3 p-5">
      <span
        className={cn(
          "flex items-center gap-1.5 font-medium text-muted-foreground text-xs uppercase tracking-widest transition-colors",
          active && "text-primary"
        )}
      >
        {icon}
        {label}
        {active && (
          <span
            aria-hidden="true"
            className="size-1.5 animate-pulse rounded-full bg-primary"
          />
        )}
      </span>
      <span className="flex items-baseline gap-1.5">
        <span
          className={cn(
            "font-semibold text-4xl tabular-nums tracking-tight transition-colors",
            value === undefined && live !== undefined && "text-primary",
            shown === undefined && "text-muted-foreground/50"
          )}
        >
          {shown === undefined
            ? m.common_dash_placeholder()
            : formatMetric(shown)}
        </span>
        <span className="text-muted-foreground text-sm">Mbps</span>
      </span>
      <ThroughputChart points={points} />
    </div>
  );
}

function SmallResult({
  label,
  value,
  active,
}: {
  label: string;
  value: number | undefined;
  active: boolean;
}) {
  return (
    <div className="flex items-baseline gap-2">
      <dt
        className={cn(
          "text-muted-foreground transition-colors",
          active && "text-primary"
        )}
      >
        {label}
      </dt>
      <dd className="font-semibold tabular-nums">
        {value === undefined
          ? m.common_dash_placeholder()
          : formatMetric(value)}
        <span className="ml-1 font-normal text-muted-foreground text-xs">
          ms
        </span>
      </dd>
    </div>
  );
}

function formatMetric(value: number): string {
  const digits = value >= 100 ? 0 : 1;
  return value.toFixed(digits);
}
