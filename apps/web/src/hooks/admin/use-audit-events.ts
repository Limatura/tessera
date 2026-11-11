import { useInfiniteQuery } from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

import { adminKeys } from "./keys";

type AuditEventPage = components["schemas"]["AuditEventPage"];

/** What the page filters by; every field left out matches everything. */
export interface AuditFilters {
  action?: string;
  outcome?: string;
  actor?: string;
  target_type?: string;
  target_id?: string;
  from?: string;
  to?: string;
}

const PAGE_SIZE = 50;

/** The URL the export link points at: the same filters, as CSV. */
export function auditExportUrl(filters: AuditFilters): string {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(filters)) {
    if (value) {
      params.set(key, value);
    }
  }
  const query = params.toString();
  return `/api/v1/admin/audit-events/export${query ? `?${query}` : ""}`;
}

/**
 * The audit log, newest first, a page at a time. "Load more" follows the
 * server's cursor, so a row written meanwhile cannot shift a page.
 */
export function useAuditEvents(filters: AuditFilters) {
  return useInfiniteQuery({
    getNextPageParam: (last: AuditEventPage) => last.next_cursor ?? undefined,
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }): Promise<AuditEventPage> => {
      const { data, error } = await api.GET("/api/v1/admin/audit-events", {
        params: {
          query: { ...filters, cursor: pageParam, limit: PAGE_SIZE },
        },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: adminKeys.auditEventsList({ ...filters }),
  });
}
