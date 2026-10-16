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
 * All three endpoints accept only the cookie; a bearer token gets 403. `api`
 * sends the cookie by default, so no Authorization header is added here.
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
 * Ends one session. An already ended one also answers 204: it is not an error,
 * since either way the session no longer works, so just refetch the list.
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

/** "Sign out other devices": ends every session except this one. */
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
