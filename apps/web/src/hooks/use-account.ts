import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

import { sessionQueryOptions } from "@/hooks/use-auth";

type UpdateAccountRequest = components["schemas"]["UpdateAccountRequest"];
type DeleteAccountRequest = components["schemas"]["DeleteAccountRequest"];
type ErrorBody = components["schemas"]["ErrorBody"];

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

/** 重寄驗證信失敗時帶出的東西。429 時多一個 `retryAfterSeconds`,畫面才講得出要等多久。 */
export type VerificationRequestError = Partial<ErrorBody> & {
  retryAfterSeconds?: number;
};

/**
 * 寄驗證信到目前的 email。
 *
 * 409 代表 session 快取裡的狀態舊了(已經在別處驗證過),這時重抓 session,
 * 畫面上的「未驗證」才會自己消失。
 */
export function useRequestEmailVerification() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const { error, response } = await api.POST(
        "/api/v1/account/email/verification",
        {}
      );
      if (error) {
        const header = response.headers.get("Retry-After");
        // 跟其他 hook 一樣丟 openapi-fetch 給的 ErrorBody,只是多掛一個欄位。
        throw Object.assign(error, {
          retryAfterSeconds: header ? Number(header) : undefined,
        }) satisfies VerificationRequestError;
      }
    },
    onError: (error: VerificationRequestError) => {
      if (error.code === "email_already_verified") {
        queryClient.invalidateQueries({
          queryKey: sessionQueryOptions.queryKey,
        });
      }
    },
  });
}

/**
 * 用信裡的 token 完成驗證。不需要登入:信常常是在另一台裝置打開的。
 * 有登入的話 session 裡的 `email_verified_at` 要跟著變。
 */
export function useConfirmEmailVerification() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (token: string) => {
      const { error } = await api.POST(
        "/api/v1/account/email/verification/confirm",
        { body: { token } }
      );
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: sessionQueryOptions.queryKey });
    },
  });
}
