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
 * `includeDeleted` 預設關:刪掉的帳號只剩一列殘骸(`gone-…`),不代表任何人,
 * 也不能再對它做什麼,只有刻意要看時才列出來。
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

/** `role: null` 是「沒有角色」—— 登入了但什麼都不能做，是個真實狀態不是錯誤。 */
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
 * 停用／啟用。只送 `state` —— `is_admin` 一帶就會重寫角色(`true` 變 owner、
 * `false` 變沒有角色),停用一個人不該順手降他的級。
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
 * 單一帳號的配額，蓋過角色的設定。
 *
 * 兩個欄位都是三種指令：不帶 = 不動、`null` = 清掉改回沿用角色、`0` = 這個帳號
 * 不限制。所以呼叫端要嘛給值、要嘛整個 key 不放，不能拿 `undefined` 當 `null`。
 * 一樣不帶 `is_admin`，理由同 `useSetUserState`。
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

/** 刪帳號不會動到他的分享(server 端把 owner 設成 NULL),要清掉是另一個動作。 */
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

/** 一次結束某個人上傳的全部分享。回傳結束了幾份。 */
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
