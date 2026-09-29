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
 * apps/admin 每一頁都要權限，這裡列的是「至少有一個後台頁面進得去」的判準。順序無關 ——
 * 要決定進哪一頁是 apps/admin 自己的事（見它的 `LANDING_ORDER`，那份必須是這裡的子集）。
 *
 * apps/web 用這個決定要不要顯示後台連結：沒有這些權限的人點進去只會被 admin 的 route
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

/**
 * 「沒有角色」跟 `guest` 在權限上都是空集合，差別只在語意：前者是操作者還沒處理，
 * 後者是操作者看過了、刻意選了限制最嚴的角色。畫面上分開顯示，否則「忘了指派」會被
 * 誤讀成「已經處理過」。
 */
const ROLE_LABELS: Record<string, string> = {
  admin: "管理員",
  guest: "訪客",
  member: "一般成員",
  owner: "擁有者",
};

export const UNASSIGNED_ROLE_LABEL = "未指派";

export function roleLabel(role: string): string {
  return ROLE_LABELS[role] ?? role;
}

/** 目前一個帳號只會在 0 或 1 個角色裡（見 m20260824_000001_single_role migration）。 */
export function accountRoleLabel(account: Account | null | undefined): string {
  const role = account?.roles.at(0);
  return role ? roleLabel(role) : UNASSIGNED_ROLE_LABEL;
}

export function isUnassigned(account: Account | null | undefined): boolean {
  return (account?.roles.length ?? 0) === 0;
}
