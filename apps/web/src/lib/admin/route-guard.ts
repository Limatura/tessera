import type { QueryClient } from "@tanstack/react-query";
import { redirect } from "@tanstack/react-router";

import { requireAuth } from "@/lib/route-guard";

import type { Permission } from "./permissions";
import { hasPermission, landingRoute } from "./permissions";

/**
 * 需要特定權限的後台頁面。
 *
 * 沒登入由 `requireAuth` 送去登入頁；被要求改密碼由 root 的 `requirePasswordChanged`
 * 先擋下，走不到這裡。
 *
 * 權限不足時導去「這個人自己的第一個可進入頁面」而不是丟 403 畫面：使用者多半是點了
 * 舊書籤或別人貼的連結，把他丟在一個進不去的網址上沒有任何幫助。真正的授權由 server 擋，
 * 這裡只是不要讓畫面停在空白。
 */
export async function requirePermission(
  queryClient: QueryClient,
  href: string,
  permission: Permission
) {
  const { account } = await requireAuth(queryClient, href);

  if (!hasPermission(account, permission)) {
    const fallback = landingRoute(account);
    // 後台一頁都進不去 —— 這種帳號本來就不屬於後台，送回前台首頁。
    if (fallback === null) {
      throw redirect({ to: "/" });
    }
    // 目標本身就是進不去的那頁時直接放行，否則會在兩個 beforeLoad 之間無限轉址。
    if (fallback !== href) {
      throw redirect({ to: fallback });
    }
  }

  return { account };
}
