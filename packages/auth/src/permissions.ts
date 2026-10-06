import type { components } from "@tessera/api/schema";

export type Account = components["schemas"]["AccountResponse"];

/**
 * `resource:verb` 字串，對應 crates/tessera-core/src/permission.rs 的 `Permission::ALL`。
 *
 * server 端的 permission 集合是寫死的 enum 而非資料表 —— 資料庫存在但程式沒對應檢查的
 * 權限字串等於沒授權，所以這裡跟著列一份而不是從 API 動態拿。畫面上要顯示「所有可選權限」
 * 時仍應打 `GET /admin/permissions`，那份才帶得到 `privileged` 標記。
 */
export const PERMISSIONS = {
  invitesCreate: "invites:create",
  invitesDelete: "invites:delete",
  invitesRead: "invites:read",
  rolesRead: "roles:read",
  rolesWrite: "roles:write",
  settingsRead: "settings:read",
  settingsWrite: "settings:write",
  sharesCreate: "shares:create",
  sharesDelete: "shares:delete",
  sharesDeleteAny: "shares:delete:any",
  sharesRead: "shares:read",
  sharesReadAny: "shares:read:any",
  statsRead: "stats:read",
  tokensManage: "tokens:manage",
  usersDelete: "users:delete",
  usersRead: "users:read",
  usersWrite: "users:write",
} as const;

export type Permission = (typeof PERMISSIONS)[keyof typeof PERMISSIONS];

export function hasPermission(
  account: Account | null | undefined,
  permission: Permission
): boolean {
  return account?.permissions.includes(permission) ?? false;
}

export function hasAnyPermission(
  account: Account | null | undefined,
  permissions: readonly Permission[]
): boolean {
  return permissions.some((permission) => hasPermission(account, permission));
}

/**
 * 能讓一個帳號在後台找到落腳處的權限。
 *
 * 後台每一頁都要權限，這裡列的是「至少有一個後台頁面進得去」的判準。順序無關 ——
 * 要決定進哪一頁是後台自己的事（見 apps/web `lib/admin/permissions.ts` 的
 * `LANDING_ORDER`，那份必須是這裡的子集）。
 *
 * 前台 Header 用這個決定要不要顯示後台連結：沒有這些權限的人點進去只會被後台的 route
 * guard 原地送回來，入口乾脆不出現。
 */
export const ADMIN_ENTRY_PERMISSIONS = [
  PERMISSIONS.statsRead,
  PERMISSIONS.usersRead,
  PERMISSIONS.invitesRead,
  PERMISSIONS.rolesRead,
  PERMISSIONS.sharesReadAny,
] as const;

/** 這個帳號在後台有沒有任何一頁進得去。 */
export function canAccessAdmin(account: Account | null | undefined): boolean {
  return hasAnyPermission(account, ADMIN_ENTRY_PERMISSIONS);
}

export function isUnassigned(account: Account | null | undefined): boolean {
  return (account?.roles.length ?? 0) === 0;
}
