import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";
import { sessionQueryOptions } from "@tessera/auth/hooks";

import { adminKeys } from "./keys";

type RoleRequest = components["schemas"]["RoleRequest"];

export const rolesQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/roles", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: adminKeys.roles(),
});

export const permissionsQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/permissions", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: adminKeys.permissions(),
});

export function useRoles() {
  return useQuery(rolesQueryOptions);
}

export function usePermissions() {
  return useQuery(permissionsQueryOptions);
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
      queryClient.invalidateQueries({ queryKey: adminKeys.roles() });
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
      queryClient.invalidateQueries({ queryKey: adminKeys.roles() });
      queryClient.invalidateQueries({ queryKey: adminKeys.users() });
      queryClient.invalidateQueries({
        queryKey: sessionQueryOptions.queryKey,
      });
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
      queryClient.invalidateQueries({ queryKey: adminKeys.roles() });
      queryClient.invalidateQueries({ queryKey: adminKeys.users() });
      queryClient.invalidateQueries({
        queryKey: sessionQueryOptions.queryKey,
      });
    },
  });
}
