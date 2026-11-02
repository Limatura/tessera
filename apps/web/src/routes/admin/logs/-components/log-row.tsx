import { Badge } from "@tessera/ui/components/badge";
import { cn } from "@tessera/ui/lib/utils";

import type { LogEntry, LogLevel } from "@/lib/admin/log-entries";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";
import { useAppearanceStore } from "@/stores/appearance";

import { CopyButton } from "./copy-button";

export const LEVEL_STYLES: Record<LogLevel, string> = {
  debug: "text-muted-foreground",
  error: "text-red-600 dark:text-red-400",
  info: "text-sky-600 dark:text-sky-400",
  trace: "text-muted-foreground/70",
  warn: "text-amber-600 dark:text-amber-400",
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
        "group/row flex h-full items-center border-b hover:bg-muted/50 focus-within:bg-muted/50",
        selected && "bg-muted"
      )}
    >
      <button
        aria-label={m.logs_open_line({ time: formatTime(entry.time) })}
        className="flex min-w-0 flex-1 items-baseline gap-2 self-stretch px-2 text-left font-mono text-xs outline-none"
        onClick={() => onSelect(entry)}
        type="button"
      >
        <time
          className="shrink-0 text-muted-foreground tabular-nums"
          dateTime={entry.time}
        >
          {formatTime(entry.time)}
        </time>
        <Badge
          className={cn(
            "w-12 shrink-0 justify-center uppercase",
            LEVEL_STYLES[entry.level]
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
