import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";
import { useEffect } from "react";

import { watchManagedShare } from "@/lib/watch-managed-share";

type ShareResponse = components["schemas"]["ShareResponse"];

/**
 * The share as its uploader sees it: `GET /shares/{code}/manage`, authorised by
 * the session or the anonymous upload's management token. Unlike the public
 * description it answers for a disabled share and carries the scan verdict, so
 * the uploader can be told the share was switched off for a threat.
 *
 * While the share waits on the scanner, follows `GET /shares/{code}/manage/events`
 * and writes each pushed state into the cache, instead of polling.
 */
export function useManagedShare(
  code: string,
  managementToken: string | undefined,
  enabled = true
) {
  const queryClient = useQueryClient();
  const query = useQuery({
    enabled,
    queryFn: async (): Promise<ShareResponse> => {
      const { data, error } = await api.GET("/api/v1/shares/{code}/manage", {
        params: {
          header: managementToken
            ? { "X-Share-Management-Token": managementToken }
            : undefined,
          path: { code },
        },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: ["share", code, "manage"],
    retry: false,
  });

  const scanning = query.data?.scanning === true;
  useEffect(() => {
    if (!enabled || !scanning) {
      return;
    }
    return watchManagedShare({
      code,
      managementToken,
      onShare: (share) =>
        queryClient.setQueryData<ShareResponse>(
          ["share", code, "manage"],
          share
        ),
    });
  }, [code, enabled, managementToken, queryClient, scanning]);

  return query;
}
