import { Button } from "@tessera/ui/components/button";
import { toast } from "@tessera/ui/components/toast";
import { cn } from "@tessera/ui/lib/utils";
import {
  Check,
  CircleAlert,
  Copy,
  ExternalLink,
  FileText,
  File as FileIcon,
  Link as LinkIcon,
  X,
} from "lucide-react";

import { i18n } from "#i18n";
import { browser } from "#imports";
import { useJobs } from "@/hooks/use-storage";
import { isActive, removeJobs } from "@/lib/jobs";
import { send } from "@/lib/messages";
import type { Job } from "@/lib/storage";

export function JobList() {
  const jobs = useJobs();
  const finished = jobs.filter((job) => !isActive(job));

  if (jobs.length === 0) {
    return (
      <p className="px-1 py-6 text-center text-sm text-muted-foreground">
        {i18n.t("jobs.empty")}
      </p>
    );
  }

  const handleClear = async () => {
    try {
      await removeJobs(finished.map((job) => job.id));
    } catch {
      // Storage gone with the extension; nothing to clear.
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <ul className="flex flex-col gap-1.5">
        {jobs.map((job) => (
          <JobRow job={job} key={job.id} />
        ))}
      </ul>
      {finished.length > 0 && (
        <Button
          className="self-end"
          onClick={handleClear}
          size="xs"
          variant="ghost"
        >
          {i18n.t("jobs.clear")}
        </Button>
      )}
    </div>
  );
}

function JobRow({ job }: { job: Job }) {
  const active = isActive(job);

  const handleCancel = async () => {
    // Uploads run in the background; it is the one that can stop them.
    try {
      await send({ id: job.id, type: "cancel-upload" });
    } catch {
      // The background is restarting; it will fail the job itself.
    }
  };

  const handleCopy = async () => {
    if (!job.link) {
      return;
    }
    await navigator.clipboard.writeText(job.link);
    toast.add({ title: i18n.t("common.copied"), type: "success" });
  };

  const handleOpen = async () => {
    if (job.link) {
      await browser.tabs.create({ url: job.link });
    }
  };

  return (
    <li className="flex flex-col gap-1.5 rounded-lg border bg-card px-3 py-2">
      <div className="flex items-center gap-2">
        <KindIcon kind={job.kind} />
        <span
          className="min-w-0 flex-1 truncate text-sm font-medium"
          title={job.name}
        >
          {job.name}
        </span>
        {active && (
          <Button
            aria-label={i18n.t("jobs.cancel")}
            onClick={handleCancel}
            size="icon-xs"
            title={i18n.t("jobs.cancel")}
            variant="ghost"
          >
            <X />
          </Button>
        )}
        {job.state === "done" && job.link && (
          <>
            <Button
              aria-label={i18n.t("common.copyLink")}
              onClick={handleCopy}
              size="icon-sm"
              title={i18n.t("common.copyLink")}
              variant="ghost"
            >
              <Copy />
            </Button>
            <Button
              aria-label={i18n.t("common.open")}
              onClick={handleOpen}
              size="icon-sm"
              title={i18n.t("common.open")}
              variant="ghost"
            >
              <ExternalLink />
            </Button>
          </>
        )}
      </div>
      {active ? <ProgressBar job={job} /> : <JobOutcome job={job} />}
    </li>
  );
}

function ProgressBar({ job }: { job: Job }) {
  const preparing = job.state === "preparing";
  const percent = Math.round(job.progress);
  return (
    <div className="flex items-center gap-2">
      {/* Without a value while preparing: the browser draws it as busy. */}
      <progress
        aria-label={job.name}
        className={cn(
          "h-1.5 flex-1 appearance-none overflow-hidden rounded-full bg-muted",
          "[&::-webkit-progress-bar]:bg-muted [&::-webkit-progress-value]:rounded-full [&::-webkit-progress-value]:bg-primary [&::-webkit-progress-value]:transition-[width] [&::-moz-progress-bar]:rounded-full [&::-moz-progress-bar]:bg-primary",
          preparing && "animate-pulse"
        )}
        max={100}
        value={preparing ? undefined : percent}
      />
      <span className="w-20 text-right text-xs text-muted-foreground tabular-nums">
        {preparing ? i18n.t("jobs.preparing") : `${percent}%`}
      </span>
    </div>
  );
}

function JobOutcome({ job }: { job: Job }) {
  switch (job.state) {
    case "done": {
      return (
        <p className="flex items-center gap-1 text-xs text-muted-foreground">
          <Check className="size-3.5 text-primary" />
          <span className="truncate">{job.link}</span>
        </p>
      );
    }
    case "failed": {
      return (
        <p className="flex items-start gap-1 text-xs text-destructive">
          <CircleAlert className="mt-px size-3.5 shrink-0" />
          <span>{job.error ?? i18n.t("errors.unknown")}</span>
        </p>
      );
    }
    default: {
      return (
        <p className="text-xs text-muted-foreground">
          {i18n.t("jobs.cancelled")}
        </p>
      );
    }
  }
}

function KindIcon({ kind }: { kind: Job["kind"] }) {
  const className = "size-4 shrink-0 text-muted-foreground";
  switch (kind) {
    case "note": {
      return <FileText aria-hidden className={className} />;
    }
    case "link": {
      return <LinkIcon aria-hidden className={className} />;
    }
    default: {
      return <FileIcon aria-hidden className={className} />;
    }
  }
}
