import type { Account } from "@tessera/auth/permissions";
import { hasPermission, PERMISSIONS } from "@tessera/auth/permissions";

export {
  accountRoleLabel,
  ADMIN_ENTRY_PERMISSIONS,
  canAccessAdmin,
  hasAnyPermission,
  hasPermission,
  isUnassigned,
  PERMISSIONS,
  roleLabel,
  UNASSIGNED_ROLE_LABEL,
} from "@tessera/auth/permissions";
export type { Account, Permission } from "@tessera/auth/permissions";

/**
 * 登入後往下找第一個進得去的頁面。
 *
 * 權限少的人進儀表板只會看到一片空白，所以每個畫面除了「需要什麼權限」還要有順序。
 * 這份是路由表，屬於這個 app —— 共用的只有「哪些權限算後台入口」那份清單
 * (`ADMIN_ENTRY_PERMISSIONS`)，apps/web 拿它決定要不要顯示後台連結。
 * 兩份要一起改：這裡漏掉的權限會讓有那個權限的人被 `requirePermission` 原地彈掉。
 */
const LANDING_ORDER = [
  { permission: PERMISSIONS.statsRead, to: "/dashboard" },
  { permission: PERMISSIONS.usersRead, to: "/users" },
  { permission: PERMISSIONS.invitesRead, to: "/invites" },
  { permission: PERMISSIONS.rolesRead, to: "/roles" },
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
