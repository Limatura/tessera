import { createEnv } from "@t3-oss/env-core";

/**
 * 前端的環境變數。
 *
 * `VITE_` 開頭的才讀得到。
 */
export const env = createEnv({
  // 目前沒有前端變數：後台併進 apps/web 之後，前後台互連的位址
  // （`VITE_FRONTEND_URL`、`VITE_ADMIN_URL`）都變成同一個 router 裡的路由。
  client: {},
  clientPrefix: "VITE_",
  emptyStringAsUndefined: true,
  runtimeEnv: import.meta.env,
  skipValidation: !!process.env.SKIP_ENV_VALIDATION,
});
