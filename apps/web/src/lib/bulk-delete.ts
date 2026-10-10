import type { components } from "@tessera/api/schema";

type ErrorCode = components["schemas"]["ErrorCode"];

/**
 * 批次刪除的結果,「我的分享」跟後台共用的形狀:哪些刪掉了、哪些沒有和原因。
 * server 本來就逐筆回報(`DELETE /shares`、`DELETE /admin/shares`),
 * 對話框靠它只留下沒刪掉的那幾份讓人重試。
 */
export interface BulkDeleteResult {
  done: string[];
  failed: { item: string; error: { code: ErrorCode } | unknown }[];
}

/**
 * 整個請求被拒(未登入、沒權限、超過 200 筆)時,把每一筆都當成失敗、帶同一個原因。
 * 這樣對話框不用另外處理「整批失敗」:照樣顯示原因、按刪除就是重試全部。
 */
export function rejectAll(
  items: readonly string[],
  error: unknown
): BulkDeleteResult {
  return { done: [], failed: items.map((item) => ({ error, item })) };
}
