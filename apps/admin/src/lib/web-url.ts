import { env } from "@tessera/env/web";

/** 結尾斜線會讓組出來的路徑多一條，先去掉。 */
const base = env.VITE_FRONTEND_URL.replace(/\/+$/u, "");

/**
 * 組出 apps/web 上某個頁面的位址。
 *
 * admin 跟 web 是兩包各自獨立的 SPA，被反向代理放在同一個網域下。admin 的 router
 * 裡沒有前台的路由，要連過去只能給完整位址讓瀏覽器自己去要。
 *
 * @param path 以斜線開頭的前台路徑，例如 `/login`。
 */
export function webUrl(path: string): URL {
  return new URL(`${base}${path}`, window.location.origin);
}

/** 前台頁面集中在這裡，散落各處的字串之後改路徑會漏。 */
export const WEB_ROUTES = {
  home: "/",
  login: "/login",
} as const;

/** 前台登入頁，可帶上登入後要回去的位址。 */
export function loginUrl(returnTo?: string): URL {
  const url = webUrl(WEB_ROUTES.login);
  if (returnTo) {
    url.searchParams.set(
      "redirect",
      new URL(returnTo, window.location.origin).toString()
    );
  }
  return url;
}

/**
 * 前台首頁 —— 後台一頁都進不去的人送這裡。
 *
 * 不送「我的分享」是因為那頁要登入，而且首頁的匿名上傳才是這個產品對一般訪客的入口：
 * 被彈出後台的人不一定有帳號能用的東西可看，但首頁一定能用。
 */
export function homeUrl(): URL {
  return webUrl(WEB_ROUTES.home);
}
