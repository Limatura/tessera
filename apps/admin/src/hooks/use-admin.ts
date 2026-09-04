import {
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

export const usersQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/users", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: ["admin", "users"],
});

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

export function useRoles() {
  return useQuery(rolesQueryOptions);
}

export function usePermissions() {
  return useQuery(permissionsQueryOptions);
}

export function useUsers() {
  return useQuery(usersQueryOptions);
}

export function useInvites() {
  return useQuery(invitesQueryOptions);
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
      queryClient.invalidateQueries({ queryKey: usersQueryOptions.queryKey });
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
      queryClient.invalidateQueries({ queryKey: usersQueryOptions.queryKey });
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
      queryClient.invalidateQueries({ queryKey: usersQueryOptions.queryKey });
      queryClient.invalidateQueries({ queryKey: rolesQueryOptions.queryKey });
      queryClient.invalidateQueries({ queryKey: ["auth", "session"] });
    },
  });
}
