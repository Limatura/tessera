import {
  keepPreviousData,
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import { sessionQueryOptions } from "@tessera/auth/hooks";

import { MY_SHARES_KEY } from "@/hooks/use-shares";
import { PAGE_SIZE } from "@/lib/admin/page-search";

import { adminKeys } from "./keys";

/**
 * `includeDeleted` is off by default: a deleted account is a leftover row
 * (`gone-…`) that stands for no one and cannot be acted on, so it is listed
 * only when asked for.
 */
export function usersQueryOptions(offset: number, includeDeleted = false) {
  return queryOptions({
    placeholderData: keepPreviousData,
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/admin/users", {
        params: {
          query: { include_deleted: includeDeleted, limit: PAGE_SIZE, offset },
        },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: adminKeys.usersPage(offset, includeDeleted),
  });
}

export function useUsers(offset: number, includeDeleted = false) {
  return useQuery(usersQueryOptions(offset, includeDeleted));
}

/** `role: null` means no role: signed in but allowed nothing. It is a real state, not an error. */
export function useSetUserRole() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, role }: { id: string; role: string | null }) => {
      const { data, error } = await api.PUT("/api/v1/admin/users/{id}/roles", {
        body: { role },
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.users() });
      queryClient.invalidateQueries({ queryKey: adminKeys.roles() });
      queryClient.invalidateQueries({
        queryKey: sessionQueryOptions.queryKey,
      });
    },
  });
}

export type AccountState = "active" | "disabled";

/**
 * Disable or enable. Sends only `state`: including `is_admin` would rewrite the
 * role (`true` becomes owner, `false` no role), and disabling someone should not
 * also demote them.
 */
export function useSetUserState() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, state }: { id: string; state: AccountState }) => {
      const { data, error } = await api.PATCH("/api/v1/admin/users/{id}", {
        body: { state },
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.users() });
    },
  });
}

/**
 * Per-account quotas, overriding the role's.
 *
 * Both fields are three-way: absent leaves it alone, `null` clears it back to
 * the role's value, `0` means no limit for this account. So the caller either
 * gives a value or omits the key; `undefined` is not `null`. `is_admin` is left
 * out for the same reason as in `useSetUserState`.
 */
export function useSetUserLimits() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      id,
      limits,
    }: {
      id: string;
      limits: { quota_bytes?: number | null; max_file_bytes?: number | null };
    }) => {
      const { data, error } = await api.PATCH("/api/v1/admin/users/{id}", {
        body: limits,
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.users() });
    },
  });
}

/**
 * Turns off an account's two-step sign-in, for someone who lost both the app
 * and the recovery codes. They sign in with the password alone afterwards.
 */
export function useResetUserMfa() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const { error } = await api.DELETE("/api/v1/admin/users/{id}/mfa", {
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.users() });
    },
  });
}

/**
 * Deleting an account leaves its shares alone (the server sets their owner to
 * NULL); removing them is a separate action.
 */
export function useDeleteUser() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const { error } = await api.DELETE("/api/v1/admin/users/{id}", {
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.users() });
      queryClient.invalidateQueries({ queryKey: adminKeys.roles() });
    },
  });
}

/** Ends every share a user uploaded at once. Returns how many were ended. */
export function useRemoveUserShares() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const { data, error } = await api.DELETE(
        "/api/v1/admin/users/{id}/shares",
        { params: { path: { id } } }
      );
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.shares() });
      queryClient.invalidateQueries({ queryKey: MY_SHARES_KEY });
    },
  });
}
