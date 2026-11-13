import type { components } from "@tessera/api/schema";

import * as m from "@/paraglide/messages";

export type AuditEvent = components["schemas"]["AuditEventResponse"];

/**
 * What the category filter offers, and the `action` filter each sends. Mirrors
 * `crate::audit::action` on the server: a category is a set of actions there,
 * not a column.
 */
export const AUDIT_CATEGORIES = ["shares", "sign_in", "admin"] as const;
export type AuditCategory = (typeof AUDIT_CATEGORIES)[number];

export const CATEGORY_ACTIONS: Record<AuditCategory, string> = {
  admin:
    "settings.*,user.*,role.*,invite.*,api_token.*,audit.*,share.admin_delete",
  shares: "share.create,share.retrieve,share.download,share.delete",
  sign_in: "auth.*",
};

export const CATEGORY_LABELS: Record<AuditCategory, () => string> = {
  admin: m.audit_category_admin,
  shares: m.audit_category_shares,
  sign_in: m.audit_category_sign_in,
};

export const AUDIT_OUTCOMES = ["success", "failure"] as const;
export type AuditOutcome = (typeof AUDIT_OUTCOMES)[number];

export const OUTCOME_LABELS: Record<AuditOutcome, () => string> = {
  failure: m.audit_outcome_failure,
  success: m.audit_outcome_success,
};

const ACTION_LABELS: Record<string, () => string> = {
  "api_token.create": m.audit_action_api_token_create,
  "api_token.revoke": m.audit_action_api_token_revoke,
  "audit.export": m.audit_action_audit_export,
  "auth.login": m.audit_action_auth_login,
  "auth.logout": m.audit_action_auth_logout,
  "invite.create": m.audit_action_invite_create,
  "invite.revoke": m.audit_action_invite_revoke,
  "role.create": m.audit_action_role_create,
  "role.delete": m.audit_action_role_delete,
  "role.update": m.audit_action_role_update,
  "settings.update": m.audit_action_settings_update,
  "share.admin_delete": m.audit_action_share_admin_delete,
  "share.create": m.audit_action_share_create,
  "share.delete": m.audit_action_share_delete,
  "share.download": m.audit_action_share_download,
  "share.retrieve": m.audit_action_share_retrieve,
  "user.delete": m.audit_action_user_delete,
  "user.disable": m.audit_action_user_disable,
  "user.role_change": m.audit_action_user_role_change,
  "user.update": m.audit_action_user_update,
};

/** A readable name for an action; an unknown one (a newer server) as sent. */
export function actionLabel(action: string): string {
  return ACTION_LABELS[action]?.() ?? action;
}

/** Who did it, in words: the account's name, or what kind of actor it was. */
export function actorLabel(event: AuditEvent): string {
  if (event.actor) {
    return event.actor.display_name;
  }
  switch (event.actor_kind) {
    case "anonymous": {
      return m.audit_actor_anonymous();
    }
    case "system": {
      return m.audit_actor_system();
    }
    default: {
      // An account that can no longer be read: its id is all there is.
      return event.actor_user_id ?? m.audit_actor_unknown();
    }
  }
}

/** The target as `type · id`, or nothing when the action had none. */
export function targetLabel(event: AuditEvent): string | null {
  if (!event.target_type) {
    return null;
  }
  return event.target_id
    ? `${event.target_type} · ${event.target_id}`
    : event.target_type;
}
