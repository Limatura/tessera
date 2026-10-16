import type { components } from "@tessera/api/schema";

export type Account = components["schemas"]["AccountResponse"];

/**
 * `resource:verb` strings matching `Permission::ALL` in
 * crates/tessera-core/src/permission.rs.
 *
 * The server's permission set is a fixed enum, not a table: a permission string
 * stored in the database with no check in code grants nothing. So it is listed
 * here too instead of fetched from the API. A page showing every assignable
 * permission should still call `GET /admin/permissions`, which carries the
 * `privileged` flag.
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
 * Permissions that give an account somewhere to land in the admin panel.
 *
 * Every admin page needs a permission; this is the test for "at least one page
 * is reachable". Order does not matter: which page to open is the panel's call
 * (see `LANDING_ORDER` in apps/web `lib/admin/permissions.ts`, which must be a
 * subset of this).
 *
 * The site header uses it to decide whether to show the admin link: without
 * these, the panel's route guard would just send the user back, so the link is
 * not shown.
 */
export const ADMIN_ENTRY_PERMISSIONS = [
  PERMISSIONS.statsRead,
  PERMISSIONS.usersRead,
  PERMISSIONS.invitesRead,
  PERMISSIONS.rolesRead,
  PERMISSIONS.sharesReadAny,
  PERMISSIONS.settingsRead,
] as const;

/** Whether this account can reach any admin page. */
export function canAccessAdmin(account: Account | null | undefined): boolean {
  return hasAnyPermission(account, ADMIN_ENTRY_PERMISSIONS);
}

export function isUnassigned(account: Account | null | undefined): boolean {
  return (account?.roles.length ?? 0) === 0;
}
