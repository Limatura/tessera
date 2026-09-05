import type { QueryClient } from "@tanstack/react-query";
import { redirect } from "@tanstack/react-router";

import { sessionQueryOptions } from "@/hooks/use-auth";

/**
 * 只要登入就能進的頁面。
 *
 * `/auth/session` 401 時 `ensureQueryData` 會 throw，這裡不分辨是哪種錯誤 —— 網路斷線
 * 跟沒登入都送去登入頁，真的有登入就會自己跳回來。
 *
 * 這只是不要讓畫面停在一片空白，真正的授權一律由 server 擋。
 */
export async function requireAuth(queryClient: QueryClient, href: string) {
  try {
    return { account: await queryClient.ensureQueryData(sessionQueryOptions) };
  } catch {
    throw redirect({ search: { redirect: href }, to: "/login" });
  }
}
