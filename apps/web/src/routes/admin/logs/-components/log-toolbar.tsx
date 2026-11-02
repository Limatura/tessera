import {
  IconDownload,
  IconPlayerPause,
  IconPlayerPlay,
  IconX,
} from "@tabler/icons-react";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@tessera/ui/components/dropdown-menu";
import { Input } from "@tessera/ui/components/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { formatBytes } from "@tessera/ui/lib/format";
import { cn } from "@tessera/ui/lib/utils";
import { useEffect, useRef, useState } from "react";

import { DateTimePicker } from "@/components/admin/date-time-picker";
import { useLogFiles } from "@/hooks/admin/use-log-feed";
import type {
  LiveStatus,
  LogFeed,
  LogFilters,
} from "@/hooks/admin/use-log-feed";
import { LOG_LEVELS } from "@/lib/admin/log-entries";
import type { LogLevel } from "@/lib/admin/log-entries";
import * as m from "@/paraglide/messages";

const ANY_LEVEL = "all";
const SEARCH_DEBOUNCE_MS = 300;
const FILE_URL = "/api/v1/admin/logs/files";

const LEVEL_LABELS: Record<LogLevel, () => string> = {
  debug: m.logs_level_debug,
  error: m.logs_level_error,
  info: m.logs_level_info,
  trace: m.logs_level_trace,
  warn: m.logs_level_warn,
};

const STATUS_LABELS: Record<LiveStatus, () => string> = {
  closed: m.logs_status_closed,
  connecting: m.logs_status_connecting,
  history: m.logs_status_history,
  live: m.logs_status_live,
  reconnecting: m.logs_status_reconnecting,
};

interface LogToolbarProps {
  feed: LogFeed;
  filters: LogFilters;
  onFiltersChange: (next: Partial<LogFilters>) => void;
  paused: boolean;
  onPausedChange: (paused: boolean) => void;
}

export function LogToolbar({
  feed,
  filters,
  onFiltersChange,
  paused,
  onPausedChange,
}: LogToolbarProps) {
  const [query, setQuery] = useState(filters.q);

  // The feed re-renders the page several times a second, and each render makes a
  // new `onFiltersChange`. Left in the deps it would restart the debounce every
  // time, and a typed search would never be sent.
  const latestOnChange = useRef(onFiltersChange);
  useEffect(() => {
    latestOnChange.current = onFiltersChange;
  });

  // Typing waits for a pause before it reaches the URL, and with it the server.
  useEffect(() => {
    if (query === filters.q) {
      return;
    }
    const timer = setTimeout(
      () => latestOnChange.current({ q: query }),
      SEARCH_DEBOUNCE_MS
    );
    return () => clearTimeout(timer);
  }, [query, filters.q]);

  const hasRange = filters.since !== null || filters.until !== null;

  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2">
      <StatusBadge status={feed.status} />

      <Select
        onValueChange={(value) =>
          onFiltersChange({
            level: value === ANY_LEVEL ? null : (value as LogLevel),
          })
        }
        value={filters.level ?? ANY_LEVEL}
      >
        <SelectTrigger aria-label={m.logs_level_label()} className="w-32">
          <SelectValue>
            {(value: string) =>
              value === ANY_LEVEL
                ? m.logs_level_all()
                : LEVEL_LABELS[value as LogLevel]()
            }
          </SelectValue>
        </SelectTrigger>
        <SelectContent alignItemWithTrigger={false}>
          <SelectGroup>
            <SelectItem value={ANY_LEVEL}>{m.logs_level_all()}</SelectItem>
            {LOG_LEVELS.map((level) => (
              <SelectItem key={level} value={level}>
                {LEVEL_LABELS[level]()}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>

      <Input
        aria-label={m.logs_search_label()}
        className="w-full sm:w-64"
        onChange={(event) => setQuery(event.target.value)}
        placeholder={m.logs_search_placeholder()}
        type="search"
        value={query}
      />

      <div className="grid w-full grid-cols-[1fr_auto_1fr_auto] items-center gap-1 sm:flex sm:w-auto">
        <DateTimePicker
          className="w-full sm:w-auto"
          label={m.logs_since_label()}
          onChange={(since) => onFiltersChange({ since })}
          value={filters.since}
        />
        <span aria-hidden="true">–</span>
        <DateTimePicker
          className="w-full sm:w-auto"
          label={m.logs_until_label()}
          onChange={(until) => onFiltersChange({ until })}
          value={filters.until}
        />
        {hasRange ? (
          <Button
            aria-label={m.logs_clear_range()}
            onClick={() => onFiltersChange({ since: null, until: null })}
            size="icon"
            type="button"
            variant="ghost"
          >
            <IconX />
          </Button>
        ) : (
          <span className="size-8 sm:hidden" />
        )}
      </div>

      <div className="ml-auto flex items-center gap-2">
        {feed.status !== "history" && (
          <Button
            onClick={() => onPausedChange(!paused)}
            type="button"
            variant="outline"
          >
            {paused ? <IconPlayerPlay /> : <IconPlayerPause />}
            {resumeLabel(paused, feed.pendingCount)}
          </Button>
        )}
        <FilesMenu />
      </div>
    </div>
  );
}

function resumeLabel(paused: boolean, pending: number): string {
  if (!paused) {
    return m.logs_pause();
  }
  return pending > 0
    ? m.logs_resume_pending({ count: pending })
    : m.logs_resume();
}

function StatusBadge({ status }: { status: LiveStatus }) {
  return (
    <Badge
      className="gap-1.5"
      variant={status === "live" ? "secondary" : "outline"}
    >
      <span
        aria-hidden="true"
        className={cn(
          "size-1.5 rounded-full",
          status === "live" && "bg-green-500",
          (status === "connecting" || status === "reconnecting") &&
            "animate-pulse bg-amber-500",
          status === "closed" && "bg-red-500",
          status === "history" && "bg-muted-foreground"
        )}
      />
      {STATUS_LABELS[status]()}
    </Badge>
  );
}

/** One entry per day kept; a plain link, so the browser streams the file to disk. */
function FilesMenu() {
  const files = useLogFiles();

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button type="button" variant="outline">
            <IconDownload />
            {m.logs_files()}
          </Button>
        }
      />
      <DropdownMenuContent align="end">
        {files.data?.length === 0 && (
          <p className="px-2 py-1.5 text-muted-foreground text-sm">
            {m.logs_files_empty()}
          </p>
        )}
        {files.data?.map((file) => (
          <DropdownMenuItem
            key={file.date}
            render={
              <a download href={`${FILE_URL}/${file.date}`}>
                <span className="tabular-nums">{file.date}</span>
                <span className="ml-auto text-muted-foreground text-xs tabular-nums">
                  {formatBytes(file.size_bytes)}
                </span>
              </a>
            }
          />
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
