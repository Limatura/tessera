import {
  keepPreviousData,
  queryOptions,
  useQuery,
} from "@tanstack/react-query";
import { api } from "@tessera/api";

import { adminKeys } from "./keys";

/** Live instance figures. The trend is a separate endpoint, `/admin/stats/history`. */
export const statsQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/stats", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: adminKeys.stats(),
});

const MS_PER_DAY = 86_400_000;

/**
 * The last `days` days, one row each. Only `from` is sent; `to` defaults to the
 * server's own "today" (UTC), so a browser clock running ahead does not get a
 * 400 for asking about tomorrow.
 *
 * The previous data is kept while the range changes, so the chart does not
 * flash empty.
 */
export function statsHistoryQueryOptions(days: number) {
  return queryOptions({
    placeholderData: keepPreviousData,
    queryFn: async () => {
      const from = new Date(Date.now() - (days - 1) * MS_PER_DAY)
        .toISOString()
        .slice(0, "YYYY-MM-DD".length);
      const { data, error } = await api.GET("/api/v1/admin/stats/history", {
        params: { query: { from } },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: adminKeys.statsHistory(days),
  });
}

export function useStats() {
  return useQuery(statsQueryOptions);
}

export function useStatsHistory(days: number) {
  return useQuery(statsHistoryQueryOptions(days));
}
