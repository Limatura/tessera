import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

type Account = components["schemas"]["AccountResponse"];

/**
 * Exported (not just used by `useSession`) so a route's `beforeLoad` can
 * `context.queryClient.ensureQueryData(sessionQueryOptions)` to gate on
 * `isAuthenticated` without a separate auth store.
 */
export const sessionQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/auth/session", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: ["auth", "session"],
  retry: false,
});

export function useSession() {
  return useQuery(sessionQueryOptions);
}

export function useLogin() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: components["schemas"]["LoginRequest"]) => {
      const { data, error } = await api.POST("/api/v1/auth/login", { body });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: (account: Account) => {
      queryClient.setQueryData(sessionQueryOptions.queryKey, account);
    },
  });
}

export function useRegister() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: components["schemas"]["RegisterRequest"]) => {
      const { data, error, response } = await api.POST(
        "/api/v1/auth/register",
        { body }
      );
      if (error) {
        throw error;
      }
      // `202` means the server sent a confirmation link instead of an
      // account — nothing to sign in to yet. `201`'s body is the account;
      // narrowed explicitly because the schema's other success status (202,
      // body `()`) would otherwise widen `data`'s type here.
      return response.status === 201
        ? (data as components["schemas"]["AccountResponse"])
        : undefined;
    },
    onSuccess: (account) => {
      if (account) {
        queryClient.setQueryData(sessionQueryOptions.queryKey, account);
      }
    },
  });
}

export function useLogout() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const { error } = await api.POST("/api/v1/auth/logout", {});
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      queryClient.removeQueries({ queryKey: sessionQueryOptions.queryKey });
    },
  });
}
