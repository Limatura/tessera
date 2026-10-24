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
 * Display names of the built-in roles, translated. Custom roles have no
 * translation and show their own name.
 *
 * "No role" and `guest` are both empty permission sets but mean different
 * things: the first is an operator who has not acted yet, the second a choice
 * of the most restrictive role. They are shown apart, or "forgot to assign"
 * would read as "already handled".
 *
 * Functions instead of constants, so messages are read at call time in the
 * current language.
 */
/**
 * Reads a translation table by a name from the server. `Object.hasOwn` keeps a
 * role or permission named like an `Object.prototype` member (`toString`,
 * `constructor`) from picking up the inherited property.
 */
function lookup<T>(table: Record<string, T>, key: string): T | undefined {
  return Object.hasOwn(table, key) ? table[key] : undefined;
}

const ROLE_LABELS: Record<string, () => string> = {
  admin: m.role_admin,
  guest: m.role_guest,
  member: m.role_member,
  owner: m.role_owner,
};

export function roleLabel(role: string): string {
  return lookup(ROLE_LABELS, role)?.() ?? role;
}

export function unassignedRoleLabel(): string {
  return m.role_unassigned();
}

/** An account is in at most one role today (see the m20260824_000001_single_role migration). */
export function accountRoleLabel(account: Account | null | undefined): string {
  const role = account?.roles.at(0);
  return role ? roleLabel(role) : unassignedRoleLabel();
}

/**
 * After sign-in, the first page the user can open. A user with few permissions
 * would see a blank dashboard, so each screen has an order as well as a
 * required permission. This is the admin route table. `@tessera/auth` has a
 * separate list of which permissions count as admin entry
 * (`ADMIN_ENTRY_PERMISSIONS`), used by the site Header to decide whether to
 * show the admin link. Change both together: a permission missing here gets
 * its holders bounced by `requirePermission`.
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
 * Every admin page needs a permission, so someone with none has nowhere to land
 * here; send them to the site home, where anonymous upload works without an
 * account.
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
 * The English default descriptions the server writes when it creates the
 * built-in roles (see the server's roles / owner_role / guest_role
 * migrations). While a description is still this text, or empty, the
 * translation is shown; once the operator edits it, their text is shown.
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
  return role.is_system
    ? lookup(BUILTIN_ROLE_DESCRIPTIONS, role.name)
    : undefined;
}

/** Whether a built-in role's description is untouched (empty or the default English). */
export function hasDefaultRoleDescription(role: DescribedRole): boolean {
  const builtin = builtinDescription(role);
  return (
    builtin !== undefined &&
    (!role.description || role.description === builtin.seeded)
  );
}

/** The translated description of a built-in role; `undefined` for custom roles. */
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
  "logs:read": {
    hint: m.permission_logs_read_hint,
    label: m.permission_logs_read,
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

/**
 * A permission's display name; falls back to the code when the server adds one
 * the frontend has not translated.
 */
export function permissionLabel(permission: string): string {
  return lookup(PERMISSION_LABELS, permission)?.label() ?? permission;
}

/** One sentence on what the permission allows; `undefined` if untranslated. */
export function permissionHint(permission: string): string | undefined {
  return lookup(PERMISSION_LABELS, permission)?.hint();
}

const PERMISSION_GROUP_LABELS: Record<string, () => string> = {
  invites: m.permission_group_invites,
  logs: m.permission_group_logs,
  roles: m.permission_group_roles,
  settings: m.permission_group_settings,
  shares: m.permission_group_shares,
  stats: m.permission_group_stats,
  tokens: m.permission_group_tokens,
  users: m.permission_group_users,
};

export function permissionGroupLabel(resource: string): string {
  return lookup(PERMISSION_GROUP_LABELS, resource)?.() ?? resource;
}

/**
 * Permissions to highlight on screen. The server's `privileged` flag marks
 * anything that touches other people's data, which is most permissions, and
 * marking all of them red marks nothing. This picks the ones hardest to undo
 * once granted: deleting accounts, changing roles (which allows promoting
 * yourself), changing instance settings and deleting other people's shares.
 */
export const HIGH_RISK_PERMISSIONS: ReadonlySet<string> = new Set([
  PERMISSIONS.rolesWrite,
  PERMISSIONS.settingsWrite,
  PERMISSIONS.sharesDeleteAny,
  PERMISSIONS.usersDelete,
]);
