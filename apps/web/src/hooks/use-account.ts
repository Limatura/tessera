import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

import { sessionQueryOptions } from "@/hooks/use-auth";

type UpdateAccountRequest = components["schemas"]["UpdateAccountRequest"];
type DeleteAccountRequest = components["schemas"]["DeleteAccountRequest"];

/**
 * 改自己的名稱或 email。回應就是改完的帳號,直接蓋掉 session 快取 ——
 * 改 email 會讓 server 清掉驗證狀態,畫面要看的是這份新的,不是舊的。
 */
export function useUpdateAccount() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: UpdateAccountRequest) => {
      const { data, error } = await api.PATCH("/api/v1/account", { body });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: (account) => {
      queryClient.setQueryData(sessionQueryOptions.queryKey, account);
    },
  });
}

/**
 * 刪自己。成功後 session 已經不存在,跟登出一樣把整個快取清掉,
 * 理由見 `useLogout`。
 */
export function useDeleteAccount() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: DeleteAccountRequest) => {
      const { error } = await api.DELETE("/api/v1/account", { body });
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      queryClient.clear();
    },
  });
}
