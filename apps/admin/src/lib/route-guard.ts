import type { QueryClient } from "@tanstack/react-query";
import { redirect } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";

import { sessionQueryOptions } from "@/hooks/use-auth";

import type { Permission } from "./permissions";
import { hasPermission, landingRoute } from "./permissions";
import { loginUrl } from "./web-url";

type Account = components["schemas"]["AccountResponse"];

/**
 * 把人送去登入頁。
 *
 * 登入頁屬於 apps/web，跟 admin 是兩包各自獨立的 SPA，只是被反向代理放在同一個網域下
 * （session cookie 因此共用）。admin 的 router 裡沒有那個路由，所以走 `href` ——
 * 給絕對位址時 router 會自己判斷成整頁導航。
 *
 * 帶上 `redirect` 讓 web 登完能把人送回原本要去的頁面。
 */
function toLogin(href: string): never {
  throw redirect({ href: loginUrl(href).toString() });
}

/**
 * 拿到目前登入的帳號，沒登入就轉去登入頁。
 *
 * `/auth/session` 401 時 `ensureQueryData` 會 throw，這裡不分辨是哪種錯誤 —— 網路斷線
 * 跟沒登入都送去登入頁，真的有登入就會自己跳回來。
 */
async function requireAccount(
  queryClient: QueryClient,
  href: string
): Promise<Account> {
  try {
    return await queryClient.ensureQueryData(sessionQueryOptions);
  } catch {
    return toLogin(href);
  }
}

/** 只要登入就能進的頁面。 */
export async function requireAuth(queryClient: QueryClient, href: string) {
  const account = await requireAccount(queryClient, href);
  return { account };
}

/**
 * 需要特定權限的頁面。
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
  const account = await requireAccount(queryClient, href);

  if (!hasPermission(account, permission)) {
    const fallback = landingRoute(account);
    // 目標本身就是進不去的那頁時直接放行，否則會在兩個 beforeLoad 之間無限轉址。
    if (fallback !== href) {
      throw redirect({ to: fallback });
    }
  }

  return { account };
}
