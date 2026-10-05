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
