import { api } from "@tessera/api";

import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

/**
 * What one read concluded. On success it is the values to put in the form, not
 * the full document; the endpoint returns only these fields, and the server
 * reads the rest itself at sign-in (see `auth/oidc/discovery.rs`).
 */
export type ReadResult =
  | {
      ok: true;
      /** The issuer the document states. It is authoritative, not derived from the URL. */
      issuer: string;
      /**
       * Suggested scopes reconciled with `scopes_supported`; null if the
       * provider declares none.
       */
      scopes: string[] | null;
      /**
       * Every scope the provider declares, used to explain on screen why some
       * were added or dropped.
       */
      supportedScopes: string[] | null;
      backchannelLogout: boolean;
      /**
       * The claims the provider declares it issues; null if undeclared. Used to
       * tell whether `when_verified` can have any effect: without
       * `email_verified` it will never link anyone.
       */
      claimsSupported: string[] | null;
    }
  | { ok: false; reason: string };

/** The server always adds these two (see `effective_scopes` in config.rs). */
const ALWAYS_REQUESTED = ["openid", "email"];

/**
 * Scopes this site uses that the form does not include by default.
 *
 * Both "required groups" and "groups claim" depend on it. If the provider
 * supports it but it is not requested, the groups claim is missing from the
 * token, and setting required groups locks everyone out.
 */
const USEFUL_IF_SUPPORTED = ["groups"];

/**
 * Narrows the wanted scopes to those this provider recognises.
 *
 * Synology's `scopes_supported` is `["email", "groups", "openid"]`: no
 * `profile`, though the form has it by default. Asking for an undeclared scope
 * is at best ignored and at worst rejects the entire authorisation request with
 * nothing but `refused`.
 *
 * `openid` and `email` always stay: the server adds them whatever the form
 * says, so removing them here would only make the screen disagree with what is
 * sent.
 *
 * It also adds: if the provider supports `groups` and the form does not ask
 * for it, it is added. Without it, setting "required groups" locks everyone
 * out with an unexplained `not_permitted`.
 */
export function reconcileScopes(
  wanted: string[],
  supported: string[]
): string[] {
  const kept = wanted.filter(
    (scope) => supported.includes(scope) || ALWAYS_REQUESTED.includes(scope)
  );
  for (const required of ALWAYS_REQUESTED) {
    if (!kept.includes(required)) {
      kept.push(required);
    }
  }
  for (const useful of USEFUL_IF_SUPPORTED) {
    if (supported.includes(useful) && !kept.includes(useful)) {
      kept.push(useful);
    }
  }
  return kept;
}

/**
 * Asks the server to read the provider's discovery document instead of
 * fetching it from the browser. The Discovery spec only says SHOULD about
 * browser access, and many providers (Synology DSM among them) do not allow
 * cross-origin reads, so a browser could never test them, and those are what
 * self-hosted instances connect to most. The server has no origin, so CORS
 * does not apply, and its fetch succeeding means the sign-in path will too.
 *
 * Either the issuer or the `.well-known` URL may be sent; the server handles
 * the suffix. The returned `issuer` is the one the document states, and is
 * authoritative.
 */
export async function readDiscovery(
  url: string,
  wantedScopes: string[]
): Promise<ReadResult> {
  const { data, error } = await api.POST(
    "/api/v1/admin/settings/oidc/discovery",
    { body: { url } }
  );

  if (error) {
    // The reason for `provider_unreachable` (unreachable, issuer mismatch, no
    // S256) is only in the server's message; the translation appends it as is.
    return {
      ok: false,
      reason: apiErrorMessage(error, m.settings_oidc_discovery_unreadable()),
    };
  }

  // The field is a required array, so a provider that declares nothing gets an
  // empty one. Treat that as undeclared, not "supports none", or the user's
  // scopes would be reduced to the two required ones.
  const supportedScopes =
    data.scopes_supported.length > 0 ? data.scopes_supported : null;

  return {
    backchannelLogout: data.backchannel_logout_supported,
    // As with scopes: an empty array means undeclared, not that none are issued.
    claimsSupported:
      data.claims_supported.length > 0 ? data.claims_supported : null,
    issuer: data.issuer,
    ok: true,
    // Leave the user's scopes alone when undeclared; undeclared does not mean unsupported.
    scopes: supportedScopes
      ? reconcileScopes(wantedScopes, supportedScopes)
      : null,
    supportedScopes,
  };
}
