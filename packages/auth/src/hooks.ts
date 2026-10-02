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
 * 登出。**預設把整個 query cache 清掉**，不是只清 session。
 *
 * 留著的快取是上一個帳號看得到的資料：下一個人登入後，在各自的請求回來之前會先看到
 * 那一份。兩個 app 都有這種 session 綁定的快取 —— admin 是 `/admin/*`，web 是
 * `shares/mine`、`sessions`、`preferences`。
 *
 * 代價是匿名也能看的東西（`capabilities`、以 code 取件的 `share`）會一起被丟掉，
 * 下次要用時重抓一次。拿「多一次請求」換「不會看到別人的資料」是划算的，所以這是預設。
 *
 * @param clearAllQueries 傳 `false` 可退回只清 session。除非那一頁確定沒有任何
 *   跟帳號綁定的快取，否則不要關。
 */
export function useLogout({ clearAllQueries = true } = {}) {
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

/** 寄重設信失敗時帶出的東西。429 時多一個 `retryAfterSeconds`，畫面才講得出要等多久。 */
export type PasswordResetRequestError = Partial<
  components["schemas"]["ErrorBody"]
> & {
  retryAfterSeconds?: number;
};

/**
 * 寄出密碼重設信。永遠成功（`202`）：不論帳號是否存在、信有沒有送達，
 * 都不能讓呼叫端分辨出差異，否則就變成「這個 email 有沒有註冊」的探測工具。
 * 呼叫端的成功文案要配合這點，不要暗示帳號是否存在。
 */
export function useRequestPasswordReset() {
  return useMutation({
    mutationFn: async (body: components["schemas"]["PasswordResetRequest"]) => {
      const { error, response } = await api.POST(
        "/api/v1/auth/password-reset",
        { body }
      );
      if (error) {
        const header = response.headers.get("Retry-After");
        throw Object.assign(error, {
          retryAfterSeconds: header ? Number(header) : undefined,
        }) satisfies PasswordResetRequestError;
      }
    },
  });
}

/**
 * 用註冊確認信裡的 token 把等待中的註冊變成真正的帳號。
 *
 * 成功（`201`）會帶 session cookie，所以跟 `useLogin` 一樣把回應寫進 session
 * 快取 —— 不這麼做的話，這頁導去首頁後第一個 `useSession` 還要再打一次 API
 * 才知道已經登入。
 */
export function useConfirmRegistration() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (
      body: components["schemas"]["RegisterConfirmRequest"]
    ) => {
      const { data, error } = await api.POST("/api/v1/auth/register/confirm", {
        body,
      });
      if (error) {
        throw error;
      }
      return data as Account;
    },
    onSuccess: (account) => {
      queryClient.setQueryData(sessionQueryOptions.queryKey, account);
    },
  });
}
