import { z } from "zod";

/**
 * 頁碼放 search 而不是路徑,重新整理跟複製網址都會回到同一頁。
 * 用 1-based 的 `page` 而不是 `offset`,網址對人來說才讀得懂。
 *
 * 不跟 `Pager` 放同一個檔:`validateSearch` 不會被拆 chunk,schema 放在元件檔
 * 會把整個 Pager 連帶拉進入口 bundle。
 */
export const pageSearchSchema = z.object({
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod 的 catch,不是 Promise 的
  page: z.coerce.number().int().min(1).catch(1),
});
