import { createEnv } from "@t3-oss/env-core";

/**
 * 前端的環境變數。
 *
 * `client` 目前是空的 —— 前端還沒有需要從環境讀的東西,因為 API 跟前端同源,
 * 不需要一個 base URL。之後要加的話寫在這裡,`VITE_` 開頭的才讀得到。
 */
export const env = createEnv({
  client: {},
  clientPrefix: "VITE_",
  emptyStringAsUndefined: true,
  runtimeEnv: import.meta.env,
  skipValidation: !!process.env.SKIP_ENV_VALIDATION,
});
