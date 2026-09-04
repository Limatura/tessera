import { createEnv } from "@t3-oss/env-core";
import { z } from "zod";

/**
 * 前端的環境變數。
 *
 * `VITE_` 開頭的才讀得到。
 */
export const env = createEnv({
  client: {
    /**
     * apps/web 的位址，作為前台所有頁面的基底。
     *
     * apps/admin 沒有自己的登入頁 —— 它跟 apps/web 由反向代理放在同一個網域下，
     * session cookie 本來就共用，所以未登入時是把人送去前台的登入頁。除了登入，
     * 之後要連前台的其他頁面（分享、註冊、重設密碼）也都從這個基底組出來，
     * 不要每個頁面各配一個變數。
     *
     * 同網域且掛在根路徑時留空即可（預設）；前台在子路徑就填 `/app` 這種前綴；
     * 開發時兩個 dev server 不同 port，填完整位址 `http://localhost:3001`。
     * 結尾的斜線會被忽略。
     */
    VITE_FRONTEND_URL: z.string().default(""),
    /**
     * apps/admin 的位址，讓 apps/web 組出後台管理連結（下拉選單裡的「後台管理」）。
     *
     * 兩包 SPA 由反向代理放在同一個網域下，session cookie 共用，所以連過去不用
     * 額外登入。同網域且後台掛在固定子路徑時填前綴，例如 `/admin`；開發時兩個
     * dev server 不同 port，填完整位址 `http://localhost:3002`。留空則不顯示
     * 後台連結（視為沒有設定後台）。
     */
    VITE_ADMIN_URL: z.string().default(""),
  },
  clientPrefix: "VITE_",
  emptyStringAsUndefined: true,
  runtimeEnv: import.meta.env,
  skipValidation: !!process.env.SKIP_ENV_VALIDATION,
});
