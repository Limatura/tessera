import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

import type { Account } from "./permissions";

/**
 * 匯出（而不是只給 `useSession` 用）是為了讓路由的 `beforeLoad` 能用
 * `context.queryClient.ensureQueryData(sessionQueryOptions)` 在畫面掛載前就拿到權限，
 * 不必另外開一個 auth store。
 *
 * `retry: false`：401 是「沒登入」這個正常狀態，不是需要重試的失敗。
 */
export const sessionQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/auth/session", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: ["auth", "session"],
  retry: false,
});

export function useSession() {
  return useQuery(sessionQueryOptions);
}

export function useLogin() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: components["schemas"]["LoginRequest"]) => {
      const { data, error } = await api.POST("/api/v1/auth/login", { body });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: (account: Account) => {
      queryClient.setQueryData(sessionQueryOptions.queryKey, account);
    },
  });
}

export function useRegister() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: components["schemas"]["RegisterRequest"]) => {
      const { data, error, response } = await api.POST(
        "/api/v1/auth/register",
        { body }
      );
      if (error) {
        throw error;
      }
      // `202` means the server sent a confirmation link instead of an
      // account — nothing to sign in to yet. `201`'s body is the account;
      // narrowed explicitly because the schema's other success status (202,
      // body `()`) would otherwise widen `data`'s type here.
      return response.status === 201 ? (data as Account) : undefined;
    },
    onSuccess: (account) => {
      if (account) {
        queryClient.setQueryData(sessionQueryOptions.queryKey, account);
      }
    },
  });
}

/**
 * @param clearAllQueries 登出後把整個 query cache 清掉，而不是只清 session。
 *
 * apps/admin 要這個：留著的 `/admin/*` 快取是上一個帳號看得到的資料，下一個人登入後
 * 在請求回來之前會先看到那份。apps/web 的快取沒有這種跨帳號外洩的問題，清 session
 * 就夠，整包清掉反而會把匿名也能看的東西一起丟了。
 */
export function useLogout({ clearAllQueries = false } = {}) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const { error } = await api.POST("/api/v1/auth/logout", {});
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      if (clearAllQueries) {
        queryClient.clear();
      } else {
        queryClient.removeQueries({ queryKey: sessionQueryOptions.queryKey });
      }
    },
  });
}

export function usePasswordReset() {
  return useMutation({
    mutationFn: async (
      body: components["schemas"]["PasswordResetConfirmRequest"]
    ) => {
      const { data, error } = await api.POST(
        "/api/v1/auth/password-reset/confirm",
        { body }
      );
      if (error) {
        throw error;
      }
      return data;
    },
  });
}
