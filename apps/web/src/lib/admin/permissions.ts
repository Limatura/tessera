import type { Account } from "@tessera/auth/permissions";
import { hasPermission, PERMISSIONS } from "@tessera/auth/permissions";

import * as m from "@/paraglide/messages";

export {
  ADMIN_ENTRY_PERMISSIONS,
  canAccessAdmin,
  hasAnyPermission,
  hasPermission,
  isUnassigned,
  PERMISSIONS,
} from "@tessera/auth/permissions";
export type { Account, Permission } from "@tessera/auth/permissions";

/**
 * 內建角色的顯示名稱，跟著語系翻譯（#388）。自訂角色沒有翻譯，照原名顯示。
 *
 * 「沒有角色」跟 `guest` 在權限上都是空集合，差別只在語意：前者是操作者還沒處理，
 * 後者是操作者看過了、刻意選了限制最嚴的角色。畫面上分開顯示，否則「忘了指派」會被
 * 誤讀成「已經處理過」。
 *
 * 用函式而不是常數：訊息要在呼叫當下讀，才會是目前的語系。
 */
const ROLE_LABELS: Record<string, () => string> = {
  admin: m.role_admin,
  guest: m.role_guest,
  member: m.role_member,
  owner: m.role_owner,
};

export function roleLabel(role: string): string {
  return ROLE_LABELS[role]?.() ?? role;
}

export function unassignedRoleLabel(): string {
  return m.role_unassigned();
}

/** 目前一個帳號只會在 0 或 1 個角色裡（見 m20260824_000001_single_role migration）。 */
export function accountRoleLabel(account: Account | null | undefined): string {
  const role = account?.roles.at(0);
  return role ? roleLabel(role) : unassignedRoleLabel();
}

/**
 * 登入後往下找第一個進得去的頁面。
 *
 * 權限少的人進儀表板只會看到一片空白，所以每個畫面除了「需要什麼權限」還要有順序。
 * 這份是後台的路由表 —— 放在 `@tessera/auth` 的只有「哪些權限算後台入口」那份清單
 * (`ADMIN_ENTRY_PERMISSIONS`)，前台 Header 拿它決定要不要顯示後台連結。
 * 兩份要一起改：這裡漏掉的權限會讓有那個權限的人被 `requirePermission` 原地彈掉。
 */
const LANDING_ORDER = [
  { permission: PERMISSIONS.statsRead, to: "/admin/dashboard" },
  { permission: PERMISSIONS.usersRead, to: "/admin/users" },
  { permission: PERMISSIONS.invitesRead, to: "/admin/invites" },
  { permission: PERMISSIONS.rolesRead, to: "/admin/roles" },
  { permission: PERMISSIONS.sharesReadAny, to: "/admin/all-shares" },
  { permission: PERMISSIONS.settingsRead, to: "/admin/settings" },
] as const;

/**
 * 後台每一頁都要權限，一個權限都沒有的人在這裡沒有落腳處 —— 送回前台首頁，
 * 匿名上傳在那邊，不需要帳號也能用。
 */
export function landingRoute(
  account: Account | null | undefined
): string | null {
  const match = LANDING_ORDER.find((entry) =>
    hasPermission(account, entry.permission)
  );
  return match?.to ?? null;
}

/**
 * 後端建立內建角色時寫進資料庫的英文預設說明（見 server 的 roles / owner_role /
 * guest_role migration）。說明還是這份文字、或被清空時，畫面改顯示翻譯；
 * 操作者自己改過才照原文顯示。
 */
const BUILTIN_ROLE_DESCRIPTIONS: Record<
  string,
  { seeded: string; label: () => string }
> = {
  admin: {
    label: m.role_admin_description,
    seeded:
      "Help run the instance: accounts, invites, and looking at what is here.",
  },
  guest: {
    label: m.role_guest_description,
    seeded: "Signed in, and allowed nothing.",
  },
  member: {
    label: m.role_member_description,
    seeded: "Upload, and manage what you uploaded.",
  },
  owner: {
    label: m.role_owner_description,
    seeded: "Everything, including removing accounts and content.",
  },
};

interface DescribedRole {
  name: string;
  description?: string | null;
  is_system: boolean;
}

function builtinDescription(role: DescribedRole) {
  return role.is_system ? BUILTIN_ROLE_DESCRIPTIONS[role.name] : undefined;
}

/** 內建角色的說明是不是還沒被改過（空的或等於預設英文）。 */
export function hasDefaultRoleDescription(role: DescribedRole): boolean {
  const builtin = builtinDescription(role);
  return (
    builtin !== undefined &&
    (!role.description || role.description === builtin.seeded)
  );
}

/** 內建角色的翻譯說明；自訂角色回傳 `undefined`。 */
export function defaultRoleDescription(
  role: DescribedRole
): string | undefined {
  return builtinDescription(role)?.label();
}

export function roleDescription(role: DescribedRole): string | null {
  return hasDefaultRoleDescription(role)
    ? (defaultRoleDescription(role) ?? null)
    : (role.description ?? null);
}

const PERMISSION_LABELS: Record<
  string,
  { label: () => string; hint: () => string }
> = {
  "invites:create": {
    hint: m.permission_invites_create_hint,
    label: m.permission_invites_create,
  },
  "invites:delete": {
    hint: m.permission_invites_delete_hint,
    label: m.permission_invites_delete,
  },
  "invites:read": {
    hint: m.permission_invites_read_hint,
    label: m.permission_invites_read,
  },
  "roles:read": {
    hint: m.permission_roles_read_hint,
    label: m.permission_roles_read,
  },
  "roles:write": {
    hint: m.permission_roles_write_hint,
    label: m.permission_roles_write,
  },
  "settings:read": {
    hint: m.permission_settings_read_hint,
    label: m.permission_settings_read,
  },
  "settings:write": {
    hint: m.permission_settings_write_hint,
    label: m.permission_settings_write,
  },
  "shares:create": {
    hint: m.permission_shares_create_hint,
    label: m.permission_shares_create,
  },
  "shares:delete": {
    hint: m.permission_shares_delete_hint,
    label: m.permission_shares_delete,
  },
  "shares:delete:any": {
    hint: m.permission_shares_delete_any_hint,
    label: m.permission_shares_delete_any,
  },
  "shares:read": {
    hint: m.permission_shares_read_hint,
    label: m.permission_shares_read,
  },
  "shares:read:any": {
    hint: m.permission_shares_read_any_hint,
    label: m.permission_shares_read_any,
  },
  "stats:read": {
    hint: m.permission_stats_read_hint,
    label: m.permission_stats_read,
  },
  "tokens:manage": {
    hint: m.permission_tokens_manage_hint,
    label: m.permission_tokens_manage,
  },
  "users:delete": {
    hint: m.permission_users_delete_hint,
    label: m.permission_users_delete,
  },
  "users:read": {
    hint: m.permission_users_read_hint,
    label: m.permission_users_read,
  },
  "users:write": {
    hint: m.permission_users_write_hint,
    label: m.permission_users_write,
  },
};

/** 權限的顯示名稱；server 新增了前端還沒翻譯的權限時退回代碼。 */
export function permissionLabel(permission: string): string {
  return PERMISSION_LABELS[permission]?.label() ?? permission;
}

/** 一句話說明這個權限允許做什麼；沒翻譯的回傳 `undefined`。 */
export function permissionHint(permission: string): string | undefined {
  return PERMISSION_LABELS[permission]?.hint();
}

const PERMISSION_GROUP_LABELS: Record<string, () => string> = {
  invites: m.permission_group_invites,
  roles: m.permission_group_roles,
  settings: m.permission_group_settings,
  shares: m.permission_group_shares,
  stats: m.permission_group_stats,
  tokens: m.permission_group_tokens,
  users: m.permission_group_users,
};

export function permissionGroupLabel(resource: string): string {
  return PERMISSION_GROUP_LABELS[resource]?.() ?? resource;
}

/**
 * 畫面上要特別標出來的權限。
 *
 * server 的 `privileged` 標的是「會碰到別人的東西」，大部分權限都符合，全部標紅等於沒標
 * (#376)。這裡只挑授出去後最難收拾的：刪帳號、改角色（等於能給自己升權）、改全站設定、
 * 刪別人的分享。
 */
export const HIGH_RISK_PERMISSIONS: ReadonlySet<string> = new Set([
  PERMISSIONS.rolesWrite,
  PERMISSIONS.settingsWrite,
  PERMISSIONS.sharesDeleteAny,
  PERMISSIONS.usersDelete,
]);
