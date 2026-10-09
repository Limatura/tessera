/**
 * 後台所有 query key。全部掛在 `"admin"` 底下 —— `main.tsx` 對這個前綴設了 staleTime。
 *
 * 不帶參數的是前綴,失效時用它:`adminKeys.users()` 會把每一頁一起丟掉。
 * 帶參數的是單一份快取,只給 `queryOptions` 用。
 */
export const adminKeys = {
  all: ["admin"] as const,

  invites: () => ["admin", "invites"] as const,

  ogLayoutCheck: (layout: string) =>
    ["admin", "og-layout-check", layout] as const,

  permissions: () => ["admin", "permissions"] as const,

  roles: () => ["admin", "roles"] as const,

  settings: () => ["admin", "settings"] as const,

  /** 全站分享跟「我的分享」是兩份不同的東西,key 也分開 —— 我的見 `MY_SHARES_KEY`。 */
  shares: () => ["admin", "shares"] as const,
  sharesPage: (offset: number) => ["admin", "shares", offset] as const,
  /** 儀表板上的最近幾筆。掛在全站分享底下,刪分享時會跟著失效。 */
  sharesRecent: () => ["admin", "shares", "recent"] as const,

  stats: () => ["admin", "stats"] as const,
  statsHistory: (days: number) => ["admin", "stats", "history", days] as const,

  storageBackends: () => ["admin", "storage", "backends"] as const,

  users: () => ["admin", "users"] as const,
  usersPage: (offset: number, includeDeleted: boolean) =>
    ["admin", "users", { includeDeleted, offset }] as const,
};
