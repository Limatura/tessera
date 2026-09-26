import {
  keepPreviousData,
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

type RoleRequest = components["schemas"]["RoleRequest"];

export const rolesQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/roles", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: ["admin", "roles"],
});

export const permissionsQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/permissions", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: ["admin", "permissions"],
});

/** 一頁幾筆。server 的上限是 200,沒帶 `limit` 時只回 50 筆。 */
export const PAGE_SIZE = 20;

/** 每一頁各自快取,這是它們共同的前綴 —— 失效時整串一起丟。 */
const USERS_QUERY_KEY = ["admin", "users"] as const;

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
    queryKey: [...USERS_QUERY_KEY, { includeDeleted, offset }],
  });
}

/** 全站分享跟「我的分享」是兩份不同的東西,key 也分開 —— 我的在 `["shares", "mine"]`。 */
const ALL_SHARES_QUERY_KEY = ["admin", "shares"] as const;

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
    queryKey: [...ALL_SHARES_QUERY_KEY, offset],
  });
}

export const invitesQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/invites", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: ["admin", "invites"],
});

/** 目前登入帳號自己的分享，不是全站的 —— 全站的在 `/admin/shares`。 */
export function mySharesQueryOptions(offset: number) {
  return queryOptions({
    placeholderData: keepPreviousData,
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/shares", {
        params: { query: { limit: PAGE_SIZE, offset } },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: ["shares", "mine", offset],
  });
}

export function useRoles() {
  return useQuery(rolesQueryOptions);
}

export function usePermissions() {
  return useQuery(permissionsQueryOptions);
}

export function useUsers(offset: number, includeDeleted = false) {
  return useQuery(usersQueryOptions(offset, includeDeleted));
}

export function useAllShares(offset: number) {
  return useQuery(allSharesQueryOptions(offset));
}

export function useInvites() {
  return useQuery(invitesQueryOptions);
}

export function useMyShares(offset: number) {
  return useQuery(mySharesQueryOptions(offset));
}

export function useCreateRole() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: RoleRequest) => {
      const { data, error } = await api.POST("/api/v1/admin/roles", { body });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: rolesQueryOptions.queryKey });
    },
  });
}

export function useUpdateRole() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, body }: { id: string; body: RoleRequest }) => {
      const { data, error } = await api.PUT("/api/v1/admin/roles/{id}", {
        body,
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    // 改角色權限會改到持有者的能力，自己也可能在裡面，所以 session 一起重抓。
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: rolesQueryOptions.queryKey });
      queryClient.invalidateQueries({ queryKey: USERS_QUERY_KEY });
      queryClient.invalidateQueries({ queryKey: ["auth", "session"] });
    },
  });
}

export function useDeleteRole() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const { error } = await api.DELETE("/api/v1/admin/roles/{id}", {
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: rolesQueryOptions.queryKey });
      queryClient.invalidateQueries({ queryKey: USERS_QUERY_KEY });
      queryClient.invalidateQueries({ queryKey: ["auth", "session"] });
    },
  });
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
      queryClient.invalidateQueries({ queryKey: USERS_QUERY_KEY });
      queryClient.invalidateQueries({ queryKey: rolesQueryOptions.queryKey });
      queryClient.invalidateQueries({ queryKey: ["auth", "session"] });
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
      queryClient.invalidateQueries({ queryKey: USERS_QUERY_KEY });
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
      queryClient.invalidateQueries({ queryKey: USERS_QUERY_KEY });
      queryClient.invalidateQueries({ queryKey: rolesQueryOptions.queryKey });
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
      queryClient.invalidateQueries({ queryKey: ALL_SHARES_QUERY_KEY });
      queryClient.invalidateQueries({ queryKey: ["shares", "mine"] });
    },
  });
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
      queryClient.invalidateQueries({ queryKey: ALL_SHARES_QUERY_KEY });
      queryClient.invalidateQueries({ queryKey: ["shares", "mine"] });
    },
  });
}
