import {
  keepPreviousData,
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import { useEffect } from "react";

import { rejectAll } from "@/lib/bulk-delete";
import type { BulkDeleteResult } from "@/lib/bulk-delete";
import { watchManagedShare } from "@/lib/watch-managed-share";

/** Rows per page. The server caps it at 200; this is a number a table can show at once. */
export const SHARES_PAGE_SIZE = 20;

/**
 * Common prefix of every "My shares" page. Deleting someone's share from the
 * admin side invalidates it too.
 */
export const MY_SHARES_KEY = ["shares", "mine"] as const;

/**
 * The signed-in account's own shares. The server filters by the session's user
 * id (see `list_for_user`); the frontend neither needs to nor can choose whose.
 *
 * `offset` is in the queryKey so each page is cached separately. Paging keeps
 * the previous page as placeholder data, so the table does not vanish and
 * reappear on every page change.
 */
export function sharesQueryOptions(offset: number) {
  return queryOptions({
    placeholderData: keepPreviousData,
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/shares", {
        params: { query: { limit: SHARES_PAGE_SIZE, offset } },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: [...MY_SHARES_KEY, offset],
  });
}

/**
 * A share waiting for the scanner turns clean or blocked within seconds. Each
 * one on the page gets its own event stream, and what it pushes replaces that
 * row in the cached page.
 */
export function useMyShares(offset: number) {
  const queryClient = useQueryClient();
  const query = useQuery(sharesQueryOptions(offset));
  const scanningCodes = (query.data?.data ?? [])
    .filter((share) => share.scanning)
    .map((share) => share.code)
    .join(",");

  useEffect(() => {
    const stops = scanningCodes
      .split(",")
      .filter(Boolean)
      .map((code) =>
        watchManagedShare({
          code,
          onShare: (share) =>
            queryClient.setQueryData(
              sharesQueryOptions(offset).queryKey,
              (page) =>
                page && {
                  ...page,
                  data: page.data.map((row) =>
                    row.code === share.code ? share : row
                  ),
                }
            ),
        })
      );
    return () => {
      for (const stop of stops) {
        stop();
      }
    };
  }, [offset, queryClient, scanningCodes]);

  return query;
}

/**
 * Deletes your own shares, one or many in a single request. Returns which were
 * deleted and which were not, so on partial failure the dialog keeps the ones
 * left. Someone else's or nonexistent shares always come back as `not_found`.
 * If anything was deleted the entire `MY_SHARES_KEY` prefix is invalidated: the
 * total changed, and every later page shifts forward.
 */
export function useDeleteMyShares() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (codes: readonly string[]): Promise<BulkDeleteResult> => {
      const { data, error } = await api.DELETE("/api/v1/shares", {
        body: { codes: [...codes] },
      });
      if (error) {
        return rejectAll(codes, error);
      }
      return {
        done: data.deleted,
        failed: data.failed.map(({ code, error: reason }) => ({
          error: { code: reason },
          item: code,
        })),
      };
    },
    onSuccess: ({ done }) => {
      if (done.length > 0) {
        queryClient.invalidateQueries({ queryKey: MY_SHARES_KEY });
      }
    },
  });
}
