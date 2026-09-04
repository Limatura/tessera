import { env } from "@tessera/env/web";

/** 結尾斜線會讓組出來的路徑多一條，先去掉。 */
const adminBase = env.VITE_ADMIN_URL.replace(/\/+$/u, "");

/**
 * 組出 apps/admin 上某個頁面的位址。
 *
 * web 跟 admin 是兩包各自獨立的 SPA，被反向代理放在同一個網域下，session cookie
 * 共用。web 的 router 裡沒有後台的路由，要連過去只能給完整位址讓瀏覽器自己去要。
 *
 * `VITE_ADMIN_URL` 沒設定時視為沒有後台可連，回傳 `null`。
 *
 * @param path 以斜線開頭的後台路徑，例如 `/dashboard`。
 */
export function adminUrl(path = "/"): URL | null {
  if (!adminBase) {
    return null;
  }
  return new URL(`${adminBase}${path}`, window.location.origin);
}
