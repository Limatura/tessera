import { useLogout as useSharedLogout } from "@tessera/auth/hooks";

export { sessionQueryOptions, useSession } from "@tessera/auth/hooks";

/**
 * 登出時把整個 query cache 清掉，而不是只清 session：留著的 `/admin/*` 快取是上一個
 * 帳號看得到的資料，下一個人登入後在請求回來之前會先看到那份。
 */
export function useLogout() {
  return useSharedLogout({ clearAllQueries: true });
}
