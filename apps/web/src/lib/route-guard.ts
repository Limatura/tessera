import type { QueryClient } from "@tanstack/react-query";
import { redirect } from "@tanstack/react-router";
import { sessionQueryOptions } from "@tessera/auth/hooks";

/** 強制改密碼時唯一放行的頁面。 */
export const CHANGE_PASSWORD_PATH = "/change-password";

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

/**
 * 帳號被要求改密碼（`must_change_password`）時，除了改密碼頁哪裡都不讓進。
 *
 * 掛在 root 的 `beforeLoad`，每次換頁都會跑，所以匿名訪客不能每次都多打一次
 * `/auth/session`：快取已經是錯誤狀態（沒登入）就直接放行。登入、登出都會改寫這份
 * 快取，不會卡在舊狀態。
 *
 * server 那邊除了 session、登出、改密碼以外一律回 `403 password_change_required`，
 * 這裡只是不要讓畫面停在一堆錯誤上。
 */
export async function requirePasswordChanged(
  queryClient: QueryClient,
  location: { href: string; pathname: string }
) {
  if (location.pathname.startsWith(CHANGE_PASSWORD_PATH)) {
    return;
  }
  const { queryKey } = sessionQueryOptions;
  if (queryClient.getQueryState(queryKey)?.status === "error") {
    return;
  }
  let account;
  try {
    account = await queryClient.ensureQueryData(sessionQueryOptions);
  } catch {
    return;
  }
  if (account.must_change_password) {
    throw redirect({
      search: { redirect: location.href },
      to: CHANGE_PASSWORD_PATH,
    });
  }
}
