/**
 * Every admin query key. All live under `"admin"`, the prefix `main.tsx` sets a
 * staleTime for.
 *
 * Keys without arguments are prefixes, used for invalidation:
 * `adminKeys.users()` drops every page. Keys with arguments name one cache
 * entry and are only for `queryOptions`.
 */
export const adminKeys = {
  all: ["admin"] as const,

  invites: () => ["admin", "invites"] as const,

  ogLayoutCheck: (layout: string) =>
    ["admin", "og-layout-check", layout] as const,

  permissions: () => ["admin", "permissions"] as const,

  roles: () => ["admin", "roles"] as const,

  scanner: () => ["admin", "scanner"] as const,

  settings: () => ["admin", "settings"] as const,

  /** All shares and "my shares" are different data with separate keys; mine is `MY_SHARES_KEY`. */
  shares: () => ["admin", "shares"] as const,
  sharesPage: (offset: number) => ["admin", "shares", offset] as const,
  /**
   * The latest few on the dashboard. Nested under all shares, so deleting a
   * share invalidates it too.
   */
  sharesRecent: () => ["admin", "shares", "recent"] as const,

  stats: () => ["admin", "stats"] as const,
  statsHistory: (days: number) => ["admin", "stats", "history", days] as const,

  storageBackends: () => ["admin", "storage", "backends"] as const,

  users: () => ["admin", "users"] as const,
  usersPage: (offset: number, includeDeleted: boolean) =>
    ["admin", "users", { includeDeleted, offset }] as const,
};
