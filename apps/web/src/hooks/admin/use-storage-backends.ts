import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

import { adminKeys } from "./keys";

type BackendRequest = components["schemas"]["BackendRequest"];
type UpdateBackendRequest = components["schemas"]["UpdateBackendRequest"];

export const storageBackendsQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/storage/backends", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: adminKeys.storageBackends(),
});

export function useStorageBackends() {
  return useQuery(storageBackendsQueryOptions);
}

/**
 * 新增 backend，或用同名的請求覆蓋既有的一列（server 端的語意，見 `BackendRequest`）。
 * `url` 是 sealed 欄位：一旦送出就讀不回來，所以編輯已存在的 backend 時這裡永遠是
 * 覆蓋而非「載入後修改」——呼叫端必須自己決定要不要重新輸入 `url`。
 */
export function useAddStorageBackend() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: BackendRequest) => {
      const { data, error } = await api.POST("/api/v1/admin/storage/backends", {
        body,
      });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.storageBackends() });
    },
  });
}

/** 只能切 `direct_access`——URL 跟憑證是 sealed，改別的欄位得走 `useAddStorageBackend` 整列覆蓋。 */
export function useUpdateStorageBackend() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      name,
      body,
    }: {
      name: string;
      body: UpdateBackendRequest;
    }) => {
      const { data, error } = await api.PATCH(
        "/api/v1/admin/storage/backends/{name}",
        { body, params: { path: { name } } }
      );
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.storageBackends() });
    },
  });
}

export function useSetDefaultStorageBackend() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (name: string) => {
      const { data, error } = await api.PUT(
        "/api/v1/admin/storage/backends/{name}/default",
        { params: { path: { name } } }
      );
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.storageBackends() });
    },
  });
}

/** `409`（`backend_in_use`）代表這個後端還有檔案，由呼叫端翻譯顯示。 */
export function useRemoveStorageBackend() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (name: string) => {
      const { error } = await api.DELETE(
        "/api/v1/admin/storage/backends/{name}",
        { params: { path: { name } } }
      );
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.storageBackends() });
    },
  });
}
