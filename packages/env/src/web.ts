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
    VITE_FRONTEND_URL: z
      .string()
      .default("")
      // 結尾斜線在這裡就去掉，組路徑的地方才不用各自記得。
      .transform((value) => value.replace(/\/+$/u, ""))
      .refine(
        (value) =>
          value === "" ||
          /^https?:\/\//u.test(value) ||
          (value.startsWith("/") && !value.startsWith("//")),
        {
          error:
            "只能留空、填 `/app` 這種以單一斜線開頭的前綴，或 `http(s)://` 的完整位址",
        }
      ),
  },
  clientPrefix: "VITE_",
  emptyStringAsUndefined: true,
  runtimeEnv: import.meta.env,
  skipValidation: !!process.env.SKIP_ENV_VALIDATION,
});
