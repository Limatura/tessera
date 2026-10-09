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

export function useDeleteShare() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const { error } = await api.DELETE("/api/v1/admin/shares/{id}", {
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.shares() });
      queryClient.invalidateQueries({ queryKey: MY_SHARES_KEY });
    },
  });
}
