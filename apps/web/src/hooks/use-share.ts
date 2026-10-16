import { queryOptions, useQuery } from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

type PublicShare = components["schemas"]["PublicShareResponse"];
type Ticket = components["schemas"]["TicketResponse"];

/** How often a share still being scanned is checked again. Most files take well under a second. */
const SCANNING_POLL_MS = 3000;

/** How long to wait after a `423` without a usable `Retry-After`. */
const SCANNING_RETRY_FALLBACK_SECONDS = 3;

/** Unlock retries while scanning; past this the error surfaces instead of spinning forever. */
const SCANNING_MAX_RETRIES = 20;

interface StatusError extends Error {
  status: number;
  /** Seconds from `Retry-After`; only a `423` carries it. */
  retryAfter?: number;
}

function statusError(response: Response): StatusError {
  const retryAfter = Number(response.headers.get("Retry-After"));
  return Object.assign(new Error(response.statusText), {
    retryAfter:
      Number.isFinite(retryAfter) && retryAfter > 0 ? retryAfter : undefined,
    status: response.status,
  });
}

function isScanning(error: unknown): error is StatusError {
  return (
    typeof error === "object" &&
    error !== null &&
    "status" in error &&
    error.status === 423
  );
}

/** Does not spend a download, so it is safe for prefetching, link previews and page loads. */
export function shareQueryOptions(code: string, password?: string) {
  return queryOptions({
    queryFn: async (): Promise<PublicShare> => {
      const { data, error, response } = await api.GET("/api/v1/shares/{code}", {
        params: {
          header: password ? { "X-Share-Password": password } : undefined,
          path: { code },
        },
      });
      if (error) {
        throw statusError(response);
      }
      return data;
    },
    queryKey: ["share", code, password ?? null],
    // While `scanning: true` the share cannot be retrieved; poll until it is done.
    refetchInterval: (query) =>
      query.state.data?.scanning ? SCANNING_POLL_MS : false,
    retry: false,
  });
}

export function useShare(code: string, password?: string) {
  return useQuery(shareQueryOptions(code, password));
}

/**
 * Spends one download and caches the ticket for the rest of this visit. The
 * ticket covers every request the transfer needs, so it must not be refetched
 * silently (a refetch on window focus would waste a second use).
 *
 * No `queryOptions` by design: a route loader must not prefetch it, since that
 * would spend a download on the visitor's behalf.
 *
 * While scanning it answers `423`, and the refusal comes before a download is
 * spent, so retrying after `Retry-After` is safe.
 */
export function useShareTicket(
  code: string,
  password: string | undefined,
  enabled: boolean
) {
  return useQuery({
    enabled,
    gcTime: Number.POSITIVE_INFINITY,
    queryFn: async (): Promise<Ticket> => {
      const { data, error, response } = await api.POST(
        "/api/v1/shares/{code}/unlock",
        {
          params: {
            header: password ? { "X-Share-Password": password } : undefined,
            path: { code },
          },
        }
      );
      if (error) {
        throw statusError(response);
      }
      return data;
    },
    queryKey: ["share-ticket", code, password ?? null],
    refetchOnMount: false,
    refetchOnReconnect: false,
    refetchOnWindowFocus: false,
    retry: (failureCount, error) =>
      isScanning(error) && failureCount < SCANNING_MAX_RETRIES,
    retryDelay: (_, error) =>
      ((isScanning(error) ? error.retryAfter : undefined) ??
        SCANNING_RETRY_FALLBACK_SECONDS) * 1000,
    staleTime: Number.POSITIVE_INFINITY,
  });
}
