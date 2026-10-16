import type { QueryClient } from "@tanstack/react-query";
import { redirect } from "@tanstack/react-router";
import { sessionQueryOptions } from "@tessera/auth/hooks";

/** The only page reachable while a password change is required. */
export const CHANGE_PASSWORD_PATH = "/change-password";

/**
 * Pages that need only a sign-in.
 *
 * `ensureQueryData` throws when `/auth/session` answers 401. The kind of error
 * is not checked: offline and signed out both go to the sign-in page, and a
 * user who is signed in comes straight back.
 *
 * This only avoids a blank screen; the server enforces all authorisation.
 */
export async function requireAuth(queryClient: QueryClient, href: string) {
  try {
    return { account: await queryClient.ensureQueryData(sessionQueryOptions) };
  } catch {
    throw redirect({ search: { redirect: href }, to: "/login" });
  }
}

/**
 * Keeps an account that must change its password (`must_change_password`) on
 * the change password page.
 *
 * It runs from the root `beforeLoad` on every navigation, so an anonymous
 * visitor must not cost an extra `/auth/session` each time: a cache already in
 * the error state (signed out) passes straight through. Signing in and out both
 * rewrite this cache, so it cannot get stuck.
 *
 * The server answers `403 password_change_required` to everything except the
 * session, sign-out and password change; this only avoids a screen full of
 * errors.
 */
export async function requirePasswordChanged(
  queryClient: QueryClient,
  location: { href: string; pathname: string }
) {
  if (location.pathname.startsWith(CHANGE_PASSWORD_PATH)) {
    return;
  }
  const { queryKey } = sessionQueryOptions;
  if (queryClient.getQueryState(queryKey)?.status === "error") {
    return;
  }
  let account;
  try {
    account = await queryClient.ensureQueryData(sessionQueryOptions);
  } catch {
    return;
  }
  if (account.must_change_password) {
    throw redirect({
      search: { redirect: location.href },
      to: CHANGE_PASSWORD_PATH,
    });
  }
}
