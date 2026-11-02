import { IconArrowUp } from "@tabler/icons-react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Button } from "@tessera/ui/components/button";
import { ScrollArea } from "@tessera/ui/components/scroll-area";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { useEffect, useRef } from "react";

import { ListEmpty, ListError } from "@/components/admin/list-state";
import type { LogFeed } from "@/hooks/admin/use-log-feed";
import type { LogEntry } from "@/lib/admin/log-entries";
import { MAX_HELD_ENTRIES } from "@/lib/admin/log-entries";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

import { LogRow, useLogTimeFormat } from "./log-row";

/** Every row is this tall, so the list places rows without measuring them. */
const ROW_HEIGHT = 32;
/** Within this many pixels of the top counts as being at the top. */
const TOP_SLACK = 24;
/** Older lines are fetched when the view is this close to the end. */
const LOAD_AHEAD = 400;
const OVERSCAN = 20;
const SKELETON_ROWS = 12;

interface LogListProps {
  feed: LogFeed;
  /** The view is at the top, where new lines appear; scrolling away holds them back. */
  pinned: boolean;
  onPinnedChange: (pinned: boolean) => void;
  selectedId: string | null;
  onSelect: (entry: LogEntry) => void;
}

/**
 * The lines, newest first, with only the rows in view in the DOM. While the view
 * is at the top new lines appear there; once the reader scrolls down they are
 * held back, so what they are reading does not move, and a button brings them in.
 */
export function LogList({
  feed,
  pinned,
  onPinnedChange,
  selectedId,
  onSelect,
}: LogListProps) {
  const { entries, hasOlder, loadingOlder, loadOlder } = feed;
  const scroller = useRef<HTMLDivElement>(null);
  const formatTime = useLogTimeFormat();

  const virtualizer = useVirtualizer({
    count: entries.length,
    estimateSize: () => ROW_HEIGHT,
    getItemKey: (index) => entries[index]?.id ?? index,
    getScrollElement: () => scroller.current,
    overscan: OVERSCAN,
  });
  const totalSize = virtualizer.getTotalSize();

  // A short list never scrolls, so scrolling cannot be what asks for more.
  useEffect(() => {
    const height = scroller.current?.clientHeight ?? 0;
    if (hasOlder && !loadingOlder && totalSize > 0 && totalSize < height) {
      loadOlder();
    }
  }, [hasOlder, loadingOlder, loadOlder, totalSize]);

  // A new query starts at the top, whatever the last one was scrolled to.
  useEffect(() => {
    if (feed.loading) {
      onPinnedChange(true);
    }
  }, [feed.loading, onPinnedChange]);

  const handleScroll = () => {
    const element = scroller.current;
    if (!element) {
      return;
    }
    const atTop = element.scrollTop <= TOP_SLACK;
    if (atTop !== pinned) {
      onPinnedChange(atTop);
    }
    const distance =
      element.scrollHeight - element.scrollTop - element.clientHeight;
    if (distance < LOAD_AHEAD && hasOlder) {
      loadOlder();
    }
  };

  const handleJump = () => scroller.current?.scrollTo({ top: 0 });

  if (feed.loading) {
    return <LogSkeleton />;
  }
  if (feed.error && entries.length === 0) {
    return (
      <ListError
        isRetrying={false}
        message={apiErrorMessage(feed.error, m.logs_load_failed())}
        onRetry={feed.reload}
      />
    );
  }
  if (entries.length === 0) {
    return <ListEmpty>{m.logs_empty()}</ListEmpty>;
  }

  return (
    <div className="relative min-h-0 flex-1 overflow-hidden rounded-lg">
      {/* Absolute, so the list is exactly as big as its box and its thousands of
          rows can never stretch the page: only this area scrolls. */}
      <ScrollArea
        className="absolute inset-0 rounded-lg border"
        viewportProps={{ onScroll: handleScroll, ref: scroller, role: "log" }}
      >
        <div className="relative w-full" style={{ height: totalSize }}>
          {virtualizer.getVirtualItems().map((item) => {
            const entry = entries[item.index];
            if (!entry) {
              return null;
            }
            return (
              <div
                className="absolute top-0 left-0 w-full"
                key={item.key}
                style={{
                  height: item.size,
                  transform: `translateY(${item.start}px)`,
                }}
              >
                <LogRow
                  entry={entry}
                  formatTime={formatTime}
                  onSelect={onSelect}
                  selected={entry.id === selectedId}
                />
              </div>
            );
          })}
        </div>
        <OlderNotice feed={feed} />
      </ScrollArea>
      {!pinned && (
        <Button
          className="absolute right-4 bottom-4 rounded-full shadow-md"
          onClick={handleJump}
          type="button"
        >
          <IconArrowUp />
          {feed.pendingCount > 0
            ? m.logs_new_lines({ count: feed.pendingCount })
            : m.logs_back_to_top()}
        </Button>
      )}
    </div>
  );
}

/** Below the last line: a way to ask for more, or why there is no more to ask for. */
function OlderNotice({ feed }: { feed: LogFeed }) {
  if (feed.capped) {
    return (
      <p className="px-3 py-2 text-center text-muted-foreground text-xs">
        {m.logs_capped({ count: MAX_HELD_ENTRIES })}
      </p>
    );
  }
  if (!feed.hasOlder) {
    return null;
  }
  return (
    <div className="flex justify-center py-1">
      <Button
        disabled={feed.loadingOlder}
        onClick={feed.loadOlder}
        size="sm"
        type="button"
        variant="ghost"
      >
        {feed.loadingOlder ? m.logs_loading_older() : m.logs_load_older()}
      </Button>
    </div>
  );
}

function LogSkeleton() {
  const keys = Array.from(
    { length: SKELETON_ROWS },
    (_, index) => `row-${index}`
  );
  return (
    <output
      aria-busy="true"
      aria-label={m.common_loading()}
      className="flex flex-col gap-2 rounded-lg border p-3"
    >
      {keys.map((key) => (
        <Skeleton className="h-4 w-full" key={key} />
      ))}
    </output>
  );
}
