import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

import { adminKeys } from "./keys";

type CreateInviteRequest = components["schemas"]["CreateInviteRequest"];

export const invitesQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/invites", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: adminKeys.invites(),
});

export function useInvites() {
  return useQuery(invitesQueryOptions);
}

/** 回應裡的 `code` 只有這一次拿得到 —— server 只存它的雜湊，之後誰都讀不回來。 */
export function useCreateInvite() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: CreateInviteRequest) => {
      const { data, error } = await api.POST("/api/v1/admin/invites", {
        body,
      });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.invites() });
    },
  });
}

/** 撤銷只是讓邀請碼不能再用，已經靠它註冊的帳號不受影響。 */
export function useRevokeInvite() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const { error } = await api.DELETE("/api/v1/admin/invites/{id}", {
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: adminKeys.invites() });
    },
  });
}
