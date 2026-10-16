import * as m from "@/paraglide/messages";

/**
 * `start` and `link` both answer 303 and send the browser to the provider's
 * domain. fetch would follow the redirect and then hit CORS there, so the
 * browser has to navigate itself.
 */
function go(path: string) {
  window.location.assign(path);
}

/**
 * Starts a sign-in. An invite code is passed along; the server spends it only
 * when the account is actually created.
 *
 * `returnTo` is the path on this site to land on afterwards (for example back
 * to `/user` after signing in again to confirm identity). The server accepts
 * only a path starting with a single slash and sends anything else home.
 */
export function startSignIn(
  provider: string,
  { invite, returnTo }: { invite?: string; returnTo?: string } = {}
) {
  const query = new URLSearchParams();
  if (invite) {
    query.set("invite", invite);
  }
  if (returnTo) {
    query.set("return_to", returnTo);
  }
  const text = query.toString();
  go(
    `/api/v1/auth/${encodeURIComponent(provider)}/start${text ? `?${text}` : ""}`
  );
}

/** Links a provider to the signed-in account. The browser comes back to /user. */
export function startLink(provider: string) {
  go(`/api/v1/auth/${encodeURIComponent(provider)}/link`);
}

/**
 * On failure the server sends the browser to `/login?oidc_error=<code>` or
 * `/user?oidc_error=<code>`. The parameter name is the same for every provider.
 */
export const OIDC_ERROR_PARAM = "oidc_error";

/**
 * The ten error codes. The server sends only the code; the sentence is ours.
 *
 * An unknown value (one the server added that we have not caught up with)
 * returns null and the caller falls back to a generic failure message, since
 * a raw code means nothing to the user.
 */
export function oidcErrorMessage(code: string): string | null {
  switch (code) {
    case "unavailable": {
      return m.oidc_error_unavailable();
    }
    case "stale": {
      return m.oidc_error_stale();
    }
    case "refused": {
      return m.oidc_error_refused();
    }
    case "no_address": {
      return m.oidc_error_no_address();
    }
    case "not_permitted": {
      return m.oidc_error_not_permitted();
    }
    case "not_invited": {
      return m.oidc_error_not_invited();
    }
    case "address_taken": {
      return m.oidc_error_address_taken();
    }
    case "disabled": {
      return m.oidc_error_disabled();
    }
    case "already_linked": {
      return m.oidc_error_already_linked();
    }
    // Linking needs a sign-in within the last 10 minutes. The account page
    // draws its own sign-in-again button.
    case "sign_in_again": {
      return m.oidc_error_sign_in_again();
    }
    default: {
      return null;
    }
  }
}
