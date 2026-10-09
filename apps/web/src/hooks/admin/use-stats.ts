import {
  keepPreviousData,
  queryOptions,
  useQuery,
} from "@tanstack/react-query";
import { api } from "@tessera/api";

import { adminKeys } from "./keys";

/** 全站的即時數字。趨勢在另一支 `/admin/stats/history`。 */
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
 * 最近 `days` 天，一天一筆。只帶 `from`，`to` 讓 server 用它自己的「今天」（UTC），
 * 免得瀏覽器時鐘偏掉時要到明天的資料被回 400。
 *
 * 換範圍時留著上一份，圖不會先閃成空的。
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
