import { useQuery } from "@tanstack/react-query";
import { api } from "@tessera/api";
import { useEffect, useRef, useState } from "react";

import { adminKeys } from "@/hooks/admin/keys";
import {
  MAX_HELD_ENTRIES,
  appendOlder,
  mergeNewer,
} from "@/lib/admin/log-entries";
import type { LogEntry, LogLevel } from "@/lib/admin/log-entries";

/** What the reader asked for. `since` and `until` are RFC 3339; empty means no bound. */
export interface LogFilters {
  level: LogLevel | null;
  q: string;
  since: string | null;
  until: string | null;
}

/**
 * - `history`: a closed time range, nothing new can arrive so there is no stream
 * - `connecting`: first connection
 * - `live`: connected
 * - `reconnecting`: dropped, the browser is retrying
 * - `closed`: the server refused the stream (signed out, no permission); no retry
 */
export type LiveStatus =
  | "history"
  | "connecting"
  | "live"
  | "reconnecting"
  | "closed";

export interface LogFeed {
  entries: LogEntry[];
  /** The first page has not arrived. */
  loading: boolean;
  loadingOlder: boolean;
  error: unknown;
  /** Older lines exist on the server and fit under the cap. */
  hasOlder: boolean;
  /** Older lines exist but the cap is full. */
  capped: boolean;
  status: LiveStatus;
  /** Lines received while held back (paused, or the reader is down the list), not yet shown. */
  pendingCount: number;
  loadOlder: () => Promise<void>;
  /** Starts over with the same filters, for a failed first load. */
  reload: () => void;
}

const PAGE_SIZE = 200;
/** Stream lines are shown in batches, so a busy server does not render once per line. */
const FLUSH_MS = 250;
const STREAM_URL = "/api/v1/admin/logs/stream";

interface Held {
  entries: LogEntry[];
  /** The server has nothing older than the last held line. */
  exhausted: boolean;
}

const EMPTY: Held = { entries: [], exhausted: false };

function filterKey(filters: LogFilters): string {
  return JSON.stringify(filters);
}

function pageQuery(filters: LogFilters, cursor?: string) {
  return {
    cursor,
    level: filters.level ?? undefined,
    limit: PAGE_SIZE,
    q: filters.q || undefined,
    since: filters.since ?? undefined,
    until: filters.until ?? undefined,
  };
}

async function fetchPage(
  filters: LogFilters,
  cursor: string | undefined,
  signal: AbortSignal
) {
  const { data, error } = await api.GET("/api/v1/admin/logs", {
    params: { query: pageQuery(filters, cursor) },
    signal,
  });
  if (error) {
    throw error;
  }
  return data;
}

/**
 * The log as the page shows it, newest line first: the newest page from
 * `GET /admin/logs`, older pages on request, and, unless a closed time range is
 * set, new lines from the stream.
 *
 * Lines stay in memory only up to `MAX_HELD_ENTRIES`. The stream does not replay
 * what it missed, so every reconnect, and a `lagged` event, refetches the newest
 * page and merges it by id.
 */
export function useLogFeed(filters: LogFilters, hold: boolean): LogFeed {
  const [held, setHeld] = useState<Held>(EMPTY);
  const [loading, setLoading] = useState(true);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [status, setStatus] = useState<LiveStatus>("connecting");
  const [pendingCount, setPendingCount] = useState(0);
  const [attempt, setAttempt] = useState(0);

  const buffer = useRef<LogEntry[]>([]);
  const holdRef = useRef(hold);
  const older = useRef<AbortController | null>(null);
  const key = filterKey(filters);

  useEffect(() => {
    holdRef.current = hold;
    if (!hold && buffer.current.length > 0) {
      const batch = buffer.current;
      buffer.current = [];
      setPendingCount(0);
      setHeld((current) => ({
        ...current,
        entries: mergeNewer(current.entries, batch),
      }));
    }
  }, [hold]);

  useEffect(() => {
    const abort = new AbortController();
    const live = filters.until === null;
    buffer.current = [];
    setHeld(EMPTY);
    setPendingCount(0);
    setLoading(true);
    setError(null);
    setStatus(live ? "connecting" : "history");

    const load = async (replace: boolean) => {
      try {
        const page = await fetchPage(filters, undefined, abort.signal);
        // The page is newest first; a merge takes a batch oldest first, like the stream's.
        const newest = page.data.toReversed();
        setHeld((current) =>
          replace
            ? {
                entries: mergeNewer(current.entries, newest),
                exhausted: !page.next_cursor,
              }
            : { ...current, entries: mergeNewer(current.entries, newest) }
        );
        setError(null);
      } catch (error_) {
        if (!abort.signal.aborted) {
          setError(error_);
        }
      } finally {
        if (!abort.signal.aborted) {
          setLoading(false);
        }
      }
    };

    let source: EventSource | null = null;
    let timer: ReturnType<typeof setInterval> | undefined;

    if (live) {
      // Opened before the first page is requested, so a line logged in between
      // reaches the buffer instead of falling into the gap.
      const params = new URLSearchParams();
      if (filters.level) {
        params.set("level", filters.level);
      }
      if (filters.q) {
        params.set("q", filters.q);
      }
      source = new EventSource(`${STREAM_URL}?${params}`);
      let opened = false;

      source.addEventListener("open", () => {
        setStatus("live");
        if (opened) {
          load(false);
        }
        opened = true;
      });
      source.addEventListener("error", () => {
        setStatus(
          source?.readyState === EventSource.CLOSED ? "closed" : "reconnecting"
        );
      });
      source.addEventListener("entry", (event) => {
        buffer.current.push(JSON.parse((event as MessageEvent<string>).data));
        if (buffer.current.length > MAX_HELD_ENTRIES) {
          buffer.current = buffer.current.slice(-MAX_HELD_ENTRIES);
        }
      });
      source.addEventListener("lagged", () => {
        load(false);
      });

      timer = setInterval(() => {
        if (holdRef.current) {
          setPendingCount(buffer.current.length);
          return;
        }
        if (buffer.current.length === 0) {
          return;
        }
        const batch = buffer.current;
        buffer.current = [];
        setHeld((current) => ({
          // Trimming the oldest makes older lines reachable again.
          exhausted:
            current.exhausted &&
            current.entries.length + batch.length <= MAX_HELD_ENTRIES,
          entries: mergeNewer(current.entries, batch),
        }));
      }, FLUSH_MS);
    }

    load(true);

    return () => {
      abort.abort();
      older.current?.abort();
      source?.close();
      clearInterval(timer);
    };
    // `key` stands for `filters`, which is a new object on every render;
    // `attempt` is bumped by `reload`.
    // oxlint-disable-next-line react-hooks/exhaustive-deps
  }, [key, attempt]);

  const loadOlder = async () => {
    const oldest = held.entries.at(-1);
    if (!oldest || held.exhausted || loadingOlder || loading) {
      return;
    }
    if (held.entries.length >= MAX_HELD_ENTRIES) {
      return;
    }
    const abort = new AbortController();
    older.current = abort;
    setLoadingOlder(true);
    try {
      const page = await fetchPage(filters, oldest.id, abort.signal);
      setHeld((current) => ({
        entries: appendOlder(current.entries, page.data),
        exhausted: !page.next_cursor,
      }));
    } catch (error_) {
      if (!abort.signal.aborted) {
        setError(error_);
      }
    } finally {
      if (!abort.signal.aborted) {
        setLoadingOlder(false);
      }
    }
  };

  const full = held.entries.length >= MAX_HELD_ENTRIES;

  return {
    capped: full && !held.exhausted,
    entries: held.entries,
    error,
    hasOlder: !held.exhausted && !full,
    loadOlder,
    loading,
    loadingOlder,
    pendingCount,
    reload: () => setAttempt((current) => current + 1),
    status,
  };
}

/** The days there are files for, newest first. */
export function useLogFiles() {
  return useQuery({
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/admin/logs/files", {});
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: adminKeys.logFiles(),
  });
}
