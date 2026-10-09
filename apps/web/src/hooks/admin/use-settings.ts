import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

import { adminKeys } from "./keys";

type Settings = components["schemas"]["SettingsResponse"];
type Changes = components["schemas"]["UpdateSettingsRequest"]["changes"];

/**
 * 全站設定，一份扁平的 dotted key 清單（`share.code_length`、`features.admin`...）。
 *
 * 不可改的欄位也會回來，帶 `editable: false` 跟 `refusal` 說明為什麼 —— 少掉它們只會
 * 讓人問「port 的設定跑哪去了」。
 */
export const settingsQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/settings", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: adminKeys.settings(),
});

export function useSettings() {
  return useQuery(settingsQueryOptions);
}

/**
 * 依 dotted key 取一筆設定的值。
 *
 * 回 `unknown` 是刻意的：OpenAPI 上 `value` 是不透明的 JSON，什麼型別要看是哪個 key，
 * 由呼叫端自己比對。找不到（還在載入、或這版 server 沒這個 key）一樣是 `undefined`，
 * 所以拿它跟具體的值比對，不要拿來當布林用。
 */
export function readSetting(
  settings: Settings | undefined,
  key: string
): unknown {
  return settings?.settings.find((setting) => setting.key === key)?.value;
}

/**
 * 改設定。一次送一批，server 把它們當一組驗證，有一個不合就整批退回 ——
 * 所以只有意義上要一起生效的欄位才該放進同一次呼叫。
 */
export function useUpdateSettings() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (changes: Record<string, unknown>) => {
      const { data, error } = await api.PUT("/api/v1/admin/settings", {
        // openapi-typescript 把「任意 JSON 值」畫成 `Record<string, never>`。轉型只是
        // 繞開那個表示法，不是繞開檢查 —— server 會逐個 key 驗值，不合的整批退回。
        body: { changes: changes as Changes },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: settingsQueryOptions.queryKey,
      });
    },
  });
}
