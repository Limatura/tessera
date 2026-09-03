import type { components } from "@tessera/api/schema";

type Account = components["schemas"]["AccountResponse"];

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
 * 登入後往下找第一個進得去的頁面。
 *
 * 權限少的人進儀表板只會看到一片空白，所以每個畫面除了「需要什麼權限」還要有順序。
 * 都不符合就落到 `FALLBACK_ROUTE` —— 那頁只要登入就能看。
 */
const LANDING_ORDER = [
  { permission: PERMISSIONS.statsRead, to: "/dashboard" },
  { permission: PERMISSIONS.usersRead, to: "/users" },
  { permission: PERMISSIONS.invitesRead, to: "/invites" },
  { permission: PERMISSIONS.rolesRead, to: "/roles" },
] as const;

export const FALLBACK_ROUTE = "/shares";

export function landingRoute(account: Account | null | undefined): string {
  const match = LANDING_ORDER.find((entry) =>
    hasPermission(account, entry.permission)
  );
  return match?.to ?? FALLBACK_ROUTE;
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
