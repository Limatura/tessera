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
type ConfirmOwnerRequest = components["schemas"]["ConfirmOwnerRequest"];
type ReplaceBackendRequest = components["schemas"]["ReplaceBackendRequest"];
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

/**
 * Polled, since health changes without anyone touching the page: the server
 * re-probes a failing backend every five minutes, so a minute is often enough.
 * Paused while the tab is in the background.
 */
export function useStorageBackends() {
  return useQuery({ ...storageBackendsQueryOptions, refetchInterval: 60_000 });
}

/** Adds a backend. A name already in use is `409` (`backend_name_taken`). */
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

/**
 * Replaces an existing backend. `url` is sealed and cannot be read back, so editing
 * is a full overwrite, not load-and-modify: the caller must have the URL entered
 * again.
 */
export function useReplaceStorageBackend() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      name,
      body,
    }: {
      name: string;
      body: ReplaceBackendRequest;
    }) => {
      const { data, error } = await api.PUT(
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

/**
 * Only toggles `direct_access`. Changing anything else is a full overwrite through
 * `useReplaceStorageBackend`.
 */
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

/**
 * Removing needs the owner's password or a two-step code. `409` (`backend_in_use`)
 * means the backend still holds files; the caller translates it for display.
 */
export function useRemoveStorageBackend() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      name,
      confirm,
    }: {
      name: string;
      /** Password or two-step code, see `ConfirmOwnerRequest`. */
      confirm: ConfirmOwnerRequest;
    }) => {
      const { error } = await api.DELETE(
        "/api/v1/admin/storage/backends/{name}",
        { body: confirm, params: { path: { name } } }
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
