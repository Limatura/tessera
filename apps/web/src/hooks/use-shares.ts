import {
  keepPreviousData,
  queryOptions,
  useQuery,
} from "@tanstack/react-query";
import { api } from "@tessera/api";

/** 一頁幾筆。server 的上限是 200,這裡挑一個表格看得完的數字。 */
export const SHARES_PAGE_SIZE = 20;

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
    queryKey: ["shares", "mine", offset],
  });
}

export function useMyShares(offset: number) {
  return useQuery(sharesQueryOptions(offset));
}
