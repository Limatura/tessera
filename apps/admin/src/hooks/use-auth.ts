import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";

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

export function useLogout() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const { error } = await api.POST("/api/v1/auth/logout", {});
      if (error) {
        throw error;
      }
    },
    // 整包清掉而不是只清 session：留著的 /admin/* 快取是上一個帳號看得到的資料，
    // 下一個人登入後在請求回來之前會先看到那份。
    onSuccess: () => {
      queryClient.clear();
    },
  });
}
