import { Badge } from "@tessera/ui/components/badge";
import { cn } from "@tessera/ui/lib/utils";

import type { LogEntry, LogLevel } from "@/lib/admin/log-entries";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";
import { useAppearanceStore } from "@/stores/appearance";

import { CopyButton } from "./copy-button";

export const LEVEL_STYLES_TEXT: Record<LogLevel, string> = {
  debug: "text-slate-700 dark:text-slate-300",
  error: "text-red-700 dark:text-red-400",
  info: "text-blue-700 dark:text-blue-400",
  trace: "text-violet-700 dark:text-violet-400",
  warn: "text-amber-800 dark:text-amber-400",
};

export const LEVEL_STYLES_BG: Record<LogLevel, string> = {
  debug: "bg-slate-500/15 dark:bg-slate-400/15",
  error: "bg-red-500/20 dark:bg-red-500/20",
  info: "",
  trace: "bg-violet-500/15 dark:bg-violet-500/20",
  warn: "bg-amber-500/20 dark:bg-amber-500/20",
};

export const LEVEL_STYLES_LINE: Record<LogLevel, string> = {
  debug: "bg-slate-500 dark:bg-slate-400",
  error: "bg-red-600 dark:bg-red-400",
  info: "bg-blue-600 dark:bg-blue-400",
  trace: "bg-violet-600 dark:bg-violet-400",
  warn: "bg-amber-600 dark:bg-amber-400",
};

interface LogRowProps {
  entry: LogEntry;
  selected: boolean;
  formatTime: (iso: string) => string;
  onSelect: (entry: LogEntry) => void;
}

/**
 * One line, cut off at the edge, at a fixed height so the list can place it
 * without measuring. Pressing it opens the whole line; the copy button beside it
 * copies without opening.
 *
 * The copy button is a sibling of the row's button, not inside it: a button
 * cannot hold a button.
 */
export function LogRow({ entry, selected, formatTime, onSelect }: LogRowProps) {
  const fields = Object.entries(entry.fields);

  return (
    <div
      className={cn(
        "group/row flex h-full items-center border-b hover:bg-muted/50 focus-within:bg-muted/50 relative",
        selected && "bg-muted",
        LEVEL_STYLES_BG[entry.level]
      )}
    >
      <button
        aria-label={m.logs_open_line({ time: formatTime(entry.time) })}
        className="flex min-w-0 flex-1 items-center gap-2 self-stretch px-2 text-left font-mono text-xs outline-none"
        onClick={() => onSelect(entry)}
        type="button"
      >
        <div
          className={cn(
            "absolute left-1 top-1/2 h-4/5 w-1 -translate-y-1/2 rounded-full",
            LEVEL_STYLES_LINE[entry.level]
          )}
        />
        <time
          className="shrink-0 text-muted-foreground tabular-nums pl-3"
          dateTime={entry.time}
        >
          {formatTime(entry.time)}
        </time>
        <Badge
          className={cn(
            "w-12 shrink-0 justify-center uppercase",
            LEVEL_STYLES_TEXT[entry.level]
          )}
          variant="outline"
        >
          {entry.level}
        </Badge>
        <span className="min-w-0 flex-1 truncate">
          <span className="text-muted-foreground">{entry.target} </span>
          {entry.message}
          {fields.length > 0 && (
            <span className="text-muted-foreground">
              {" "}
              {fields
                .map(([key, value]) => `${key}=${formatValue(value)}`)
                .join(" ")}
            </span>
          )}
        </span>
      </button>
      <CopyButton
        className="mr-1 shrink-0 opacity-0 transition-opacity group-hover/row:opacity-100 focus-visible:opacity-100 pointer-coarse:opacity-100"
        label={m.logs_copy_line()}
        text={() => lineAsJson(entry)}
      />
    </div>
  );
}

export function formatValue(value: unknown): string {
  return typeof value === "string" ? value : JSON.stringify(value);
}

/** The line as the file holds it: one JSON object. */
export function lineAsJson(entry: LogEntry): string {
  return JSON.stringify(entry);
}

/**
 * Wall-clock time with milliseconds, in the reader's zone and clock format:
 * lines come close together, so the shared formatter's minutes are too coarse.
 * One formatter for the whole list, since building one per row is slow.
 */
export function useLogTimeFormat(): (iso: string) => string {
  const timeFormat = useAppearanceStore((state) => state.timeFormat);
  const formatter = new Intl.DateTimeFormat(getLocale(), {
    fractionalSecondDigits: 3,
    hour: "2-digit",
    hour12: timeFormat === "auto" ? undefined : timeFormat === "12h",
    minute: "2-digit",
    second: "2-digit",
  });
  return (iso) => formatter.format(new Date(iso));
}
