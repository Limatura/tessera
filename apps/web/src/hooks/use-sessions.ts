import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

export type Session = components["schemas"]["SessionResponse"];

const SESSIONS_KEY = ["sessions"] as const;

/**
 * 三支端點都只吃 cookie —— 帶 bearer token 會被回 403(見 issue #90)。
 * `api` 的預設就是送 cookie,所以這裡不加任何 Authorization。
 */
export function sessionsQueryOptions() {
  return queryOptions({
    queryFn: async (): Promise<Session[]> => {
      const { data, error } = await api.GET("/api/v1/sessions");
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: SESSIONS_KEY,
  });
}

export function useSessions() {
  return useQuery(sessionsQueryOptions());
}

/**
 * 結束單一 session。已經結束的那筆也回 204 —— 不是錯誤,按了它就是不能用了,
 * 所以不用特別處理,重新取清單就好。
 */
export function useRevokeSession() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (id: string) => {
      const { error } = await api.DELETE("/api/v1/sessions/{id}", {
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: SESSIONS_KEY }),
  });
}

/** 「登出其他裝置」:除了現在這個以外全部結束。 */
export function useRevokeOtherSessions() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      const { error } = await api.DELETE("/api/v1/sessions");
      if (error) {
        throw error;
      }
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: SESSIONS_KEY }),
  });
}
