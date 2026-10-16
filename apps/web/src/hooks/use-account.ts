import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";
import { sessionQueryOptions, useSession } from "@tessera/auth/hooks";

type UpdateAccountRequest = components["schemas"]["UpdateAccountRequest"];
type DeleteAccountRequest = components["schemas"]["DeleteAccountRequest"];
type ErrorBody = components["schemas"]["ErrorBody"];

/**
 * Whether this account has a password. Accounts registered only through
 * GitHub / Discord / OIDC do not, so renaming, changing email, deleting the
 * account or setting a password does not ask for one; a sign-in within the
 * last 10 minutes is required instead.
 *
 * Older servers lack the field, and back then every account had a password, so
 * a missing value counts as true.
 */
export function useHasPassword(): boolean {
  const { data: session } = useSession();
  return session?.has_password ?? true;
}

/**
 * Changes your own name or email. The response is the updated account and
 * replaces the session cache: changing the email makes the server clear its
 * verification, and the UI must show the new state.
 */
export function useUpdateAccount() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: UpdateAccountRequest) => {
      const { data, error } = await api.PATCH("/api/v1/account", { body });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: (account) => {
      queryClient.setQueryData(sessionQueryOptions.queryKey, account);
    },
  });
}

/**
 * Deletes your own account. Afterwards the session no longer exists, so the
 * whole cache is cleared as on sign-out; see `useLogout`.
 */
export function useDeleteAccount() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: DeleteAccountRequest) => {
      const { error } = await api.DELETE("/api/v1/account", { body });
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      queryClient.clear();
    },
  });
}

/**
 * What a failed verification resend carries. A 429 adds `retryAfterSeconds` so
 * the UI can say how long to wait.
 */
export type VerificationRequestError = Partial<ErrorBody> & {
  retryAfterSeconds?: number;
};

/**
 * Sends a verification email to the current address.
 *
 * 409 means the session cache is stale (already verified elsewhere), so the
 * session is refetched and the "unverified" notice goes away by itself.
 */
export function useRequestEmailVerification() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const { error, response } = await api.POST(
        "/api/v1/account/email/verification",
        {}
      );
      if (error) {
        const header = response.headers.get("Retry-After");
        // Throws openapi-fetch's ErrorBody like the other hooks, with one extra field.
        throw Object.assign(error, {
          retryAfterSeconds: header ? Number(header) : undefined,
        }) satisfies VerificationRequestError;
      }
    },
    onError: (error: VerificationRequestError) => {
      if (error.code === "email_already_verified") {
        queryClient.invalidateQueries({
          queryKey: sessionQueryOptions.queryKey,
        });
      }
    },
  });
}

/**
 * Completes verification with the token from the email. No sign-in needed: the
 * mail is often opened on another device. When signed in, the session's
 * `email_verified_at` must update too.
 */
export function useConfirmEmailVerification() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (token: string) => {
      const { error } = await api.POST(
        "/api/v1/account/email/verification/confirm",
        { body: { token } }
      );
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: sessionQueryOptions.queryKey });
    },
  });
}
