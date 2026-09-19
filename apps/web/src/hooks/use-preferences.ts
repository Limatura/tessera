import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";

import type { Appearance } from "@/lib/appearance";
import { parseAppearance } from "@/lib/appearance";

const PREFERENCES_KEY = ["preferences"] as const;

/**
 * 這個人自己選過的外觀。沒登入的人不會走到這裡 —— 偏好頁整頁都在 `requireAuth`
 * 後面,而匿名訪客照站台預設走(見 issue #260 的決定)。
 *
 * 回應裡的 `appearance` 在 OpenAPI 上是個開放的 object(utoipa 發的是 `Object`),
 * 所以在這裡 parse 成三個 enum,而不是把 `Record<string, never>` 一路帶下去。
 */
export function preferencesQueryOptions() {
  return queryOptions({
    queryFn: async (): Promise<Appearance> => {
      const { data, error } = await api.GET("/api/v1/me/preferences");
      if (error) {
        throw error;
      }
      return parseAppearance(data.appearance);
    },
    queryKey: PREFERENCES_KEY,
    // 顏色不會自己在別的地方變,重新聚焦時重抓只會製造閃爍。
    refetchOnWindowFocus: false,
  });
}

/**
 * @param enabled 沒登入時傳 `false`。端點會回 401,而 401 在這裡不是要重試或
 * 顯示的錯誤,是「這個人本來就沒有偏好」—— 乾脆不要發這個請求。
 */
export function usePreferences(enabled = true) {
  return useQuery({ ...preferencesQueryOptions(), enabled });
}

/**
 * 整包覆蓋,回新的值。
 *
 * 樂觀更新是必要的而不是裝飾:點下去到 round trip 回來之間如果不先套,使用者會
 * 看到自己選的顏色「彈回去」再變過來。失敗就把舊值放回去並讓呼叫端顯示錯誤。
 *
 * 注意送的是整個 `Appearance`,不是被改的那一個欄位 —— 端點是 PUT,少送的欄位
 * 會被當成「沒意見」清掉。
 */
export function useUpdatePreferences() {
  const queryClient = useQueryClient();

  return useMutation<Appearance, Error, Appearance, { previous?: Appearance }>({
    mutationFn: async (appearance) => {
      const { data, error } = await api.PUT("/api/v1/me/preferences", {
        // utoipa 把 `Appearance` 發成開放的 `Object`,openapi-typescript 因此
        // 譯成 `Record<string, never>` —— 一個沒有任何欄位能填的型別。實際契約
        // 是三個選填 enum,所以在這個邊界斷開,值本身已經是 `Appearance`。
        body: { appearance: appearance as Record<string, never> },
      });
      if (error) {
        throw error;
      }
      return parseAppearance(data.appearance);
    },
    onError: (_error, _appearance, context) => {
      queryClient.setQueryData(PREFERENCES_KEY, context?.previous);
    },
    onMutate: async (appearance) => {
      await queryClient.cancelQueries({ queryKey: PREFERENCES_KEY });
      const previous = queryClient.getQueryData<Appearance>(PREFERENCES_KEY);
      queryClient.setQueryData(PREFERENCES_KEY, appearance);
      return { previous };
    },
    onSuccess: (saved) => {
      queryClient.setQueryData(PREFERENCES_KEY, saved);
    },
  });
}
