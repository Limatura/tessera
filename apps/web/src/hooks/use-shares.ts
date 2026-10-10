import {
  keepPreviousData,
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";

import { rejectAll } from "@/lib/bulk-delete";
import type { BulkDeleteResult } from "@/lib/bulk-delete";

/** 一頁幾筆。server 的上限是 200,這裡挑一個表格看得完的數字。 */
export const SHARES_PAGE_SIZE = 20;

/** 「我的分享」每一頁的共同前綴。後台刪了誰的分享，也要讓這串失效。 */
export const MY_SHARES_KEY = ["shares", "mine"] as const;

/**
 * 目前登入帳號自己的分享。server 端用 session 的 user id 過濾（見 `list_for_user`），
 * 前端不需要、也不能指定要看誰的。
 *
 * `offset` 進 queryKey,每一頁各自快取;翻頁時沿用上一頁的資料當 placeholder,
 * 表格就不會在每次換頁時整塊消失再長回來。
 */
export function sharesQueryOptions(offset: number) {
  return queryOptions({
    placeholderData: keepPreviousData,
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/shares", {
        params: { query: { limit: SHARES_PAGE_SIZE, offset } },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: [...MY_SHARES_KEY, offset],
  });
}

export function useMyShares(offset: number) {
  return useQuery(sharesQueryOptions(offset));
}

/**
 * 刪自己的分享,一次一份或多份,一個請求。回傳哪些刪掉了、哪些沒有 ——
 * 部分失敗時對話框留著沒刪掉的那幾份。別人的、不存在的 server 一律回 `not_found`。
 * 只要刪掉任何一份,整個 `MY_SHARES_KEY` 前綴都失效:總數變了,後面每一頁的內容也會往前補。
 */
export function useDeleteMyShares() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (codes: readonly string[]): Promise<BulkDeleteResult> => {
      const { data, error } = await api.DELETE("/api/v1/shares", {
        body: { codes: [...codes] },
      });
      if (error) {
        return rejectAll(codes, error);
      }
      return {
        done: data.deleted,
        failed: data.failed.map(({ code, error: reason }) => ({
          error: { code: reason },
          item: code,
        })),
      };
    },
    onSuccess: ({ done }) => {
      if (done.length > 0) {
        queryClient.invalidateQueries({ queryKey: MY_SHARES_KEY });
      }
    },
  });
}
