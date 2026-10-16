import { queryOptions, useQuery } from "@tanstack/react-query";
import { api } from "@tessera/api";

import { adminKeys } from "./keys";

/**
 * The scanner's health and the backlog behind it. `settings:read`, the same as
 * the scan settings page that shows it.
 */
export const scannerQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/scanner", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: adminKeys.scanner(),
});

/**
 * Polled, since a recovery changes nothing the page itself did. The server
 * re-probes a failing scanner every five minutes, so a minute is often enough.
 * Paused while the tab is in the background.
 */
export function useScanner() {
  return useQuery({ ...scannerQueryOptions, refetchInterval: 60_000 });
}
