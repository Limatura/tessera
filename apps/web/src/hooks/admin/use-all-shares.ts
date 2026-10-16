import {
  keepPreviousData,
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";

import { MY_SHARES_KEY } from "@/hooks/use-shares";
import { PAGE_SIZE } from "@/lib/admin/page-search";
import { rejectAll } from "@/lib/bulk-delete";
import type { BulkDeleteResult } from "@/lib/bulk-delete";

import { adminKeys } from "./keys";

export function allSharesQueryOptions(offset: number) {
  return queryOptions({
    placeholderData: keepPreviousData,
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/admin/shares", {
        params: { query: { limit: PAGE_SIZE, offset } },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: adminKeys.sharesPage(offset),
  });
}

const RECENT_SHARES_LIMIT = 8;

export const recentSharesQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/shares", {
      params: { query: { limit: RECENT_SHARES_LIMIT, offset: 0 } },
    });
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: adminKeys.sharesRecent(),
});

export function useAllShares(offset: number) {
  return useQuery(allSharesQueryOptions(offset));
}

export function useRecentShares() {
  return useQuery(recentSharesQueryOptions);
}

/**
 * Deletes anyone's shares from the admin side, one or many in a single request.
 * Returns succeeded and failed lists like `useDeleteMyShares`.
 */
export function useDeleteShares() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (ids: readonly string[]): Promise<BulkDeleteResult> => {
      const { data, error } = await api.DELETE("/api/v1/admin/shares", {
        body: { ids: [...ids] },
      });
      if (error) {
        return rejectAll(ids, error);
      }
      return {
        done: data.deleted,
        failed: data.failed.map(({ id, error: reason }) => ({
          error: { code: reason },
          item: id,
        })),
      };
    },
    onSuccess: ({ done }) => {
      if (done.length > 0) {
        queryClient.invalidateQueries({ queryKey: adminKeys.shares() });
        queryClient.invalidateQueries({ queryKey: MY_SHARES_KEY });
      }
    },
  });
}
