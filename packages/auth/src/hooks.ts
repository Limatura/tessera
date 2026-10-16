import {
  hashKey,
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

import type { Account } from "./permissions";

/**
 * Exported (not just used by `useSession`) so a route's `beforeLoad` can get
 * permissions before mounting via
 * `context.queryClient.ensureQueryData(sessionQueryOptions)`, without a
 * separate auth store.
 *
 * `retry: false`: 401 is the normal "not signed in" state, not a failure to
 * retry.
 */
export const sessionQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/auth/session", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: ["auth", "session"],
  retry: false,
});

export function useSession() {
  return useQuery(sessionQueryOptions);
}

/**
 * What `POST /auth/login` can end in: signed in, or the password was right and
 * the account still wants a code from its authenticator app (`202`). The
 * ticket goes to `useLoginWithCode`.
 */
export type LoginResult =
  | { status: "signed_in"; account: Account }
  | {
      status: "mfa_required";
      challenge: components["schemas"]["MfaChallengeResponse"];
    };

export function useLogin() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (
      body: components["schemas"]["LoginRequest"]
    ): Promise<LoginResult> => {
      const { data, error, response } = await api.POST("/api/v1/auth/login", {
        body,
      });
      if (error) {
        throw error;
      }
      // Narrowed by status: `data` is the union of both success bodies.
      return response.status === 202
        ? {
            challenge: data as components["schemas"]["MfaChallengeResponse"],
            status: "mfa_required",
          }
        : { account: data as Account, status: "signed_in" };
    },
    onSuccess: (result) => {
      if (result.status === "signed_in") {
        queryClient.setQueryData(sessionQueryOptions.queryKey, result.account);
      }
    },
  });
}

/**
 * The second half of a password sign-in: the ticket from `useLogin` plus six
 * digits from the app or a recovery code.
 */
export function useLoginWithCode() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: components["schemas"]["LoginCodeRequest"]) => {
      const { data, error } = await api.POST("/api/v1/auth/login/mfa", {
        body,
      });
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
 * Signs in with whatever the browser's passkey prompt returned. A passkey
 * stands for both steps, so there is never a code to ask for afterwards.
 */
export function usePasskeySignIn() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (
      credential: components["schemas"]["AuthenticationCredential"]
    ) => {
      const { data, error } = await api.POST("/api/v1/auth/passkey", {
        body: { credential },
      });
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

export function useRegister() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: components["schemas"]["RegisterRequest"]) => {
      const { data, error, response } = await api.POST(
        "/api/v1/auth/register",
        { body }
      );
      if (error) {
        throw error;
      }
      // `202` means the server sent a confirmation link instead of creating an
      // account, so there is nothing to sign in to yet. `201`'s body is the
      // account; narrowed explicitly because the schema's other success status
      // (202, body `()`) would otherwise widen `data`'s type here.
      return response.status === 201 ? (data as Account) : undefined;
    },
    onSuccess: (account) => {
      if (account) {
        queryClient.setQueryData(sessionQueryOptions.queryKey, account);
      }
    },
  });
}

/**
 * Sign out. By default this clears the entire query cache, not just the
 * session.
 *
 * Whatever stays cached is what the previous account could see, and the next
 * person to sign in would see it until their own requests return. Both apps
 * have session-bound caches: `/admin/*` in admin, `shares/mine`, `sessions` and
 * `preferences` in web.
 *
 * The cost is that anonymous data (`capabilities`, a `share` fetched by code)
 * goes too and is refetched when next needed. One extra request for never
 * showing someone else's data is a good trade, hence the default.
 *
 * @param clearAllQueries Pass `false` to clear only the session. Do not, unless
 *   the page is certain to hold no account-bound cache.
 */
export function useLogout({ clearAllQueries = true } = {}) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const { error } = await api.POST("/api/v1/auth/logout", {});
      if (error) {
        throw error;
      }
    },
    onSuccess: () => {
      if (clearAllQueries) {
        queryClient.clear();
      } else {
        queryClient.removeQueries({ queryKey: sessionQueryOptions.queryKey });
      }
    },
  });
}

export function usePasswordReset() {
  return useMutation({
    mutationFn: async (
      body: components["schemas"]["PasswordResetConfirmRequest"]
    ) => {
      const { data, error } = await api.POST(
        "/api/v1/auth/password-reset/confirm",
        { body }
      );
      if (error) {
        throw error;
      }
      return data;
    },
  });
}

/**
 * What a failed reset-mail request carries. A 429 adds `retryAfterSeconds` so
 * the page can say how long to wait.
 */
export type PasswordResetRequestError = Partial<
  components["schemas"]["ErrorBody"]
> & {
  retryAfterSeconds?: number;
};

/**
 * Send a password reset mail. Always succeeds (`202`): whether the account
 * exists or the mail arrived must be indistinguishable to the caller, or this
 * becomes a probe for registered addresses. The caller's success text must not
 * hint either way.
 */
export function useRequestPasswordReset() {
  return useMutation({
    mutationFn: async (body: components["schemas"]["PasswordResetRequest"]) => {
      const { error, response } = await api.POST(
        "/api/v1/auth/password-reset",
        { body }
      );
      if (error) {
        const header = response.headers.get("Retry-After");
        throw Object.assign(error, {
          retryAfterSeconds: header ? Number(header) : undefined,
        }) satisfies PasswordResetRequestError;
      }
    },
  });
}

/**
 * Turn a pending registration into an account with the token from the
 * confirmation mail.
 *
 * Success (`201`) sets a session cookie, so, as in `useLogin`, the response is
 * written into the session cache. Otherwise the first `useSession` after
 * navigating home would need another API call to learn it is signed in.
 */
export function useConfirmRegistration() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (
      body: components["schemas"]["RegisterConfirmRequest"]
    ) => {
      const { data, error } = await api.POST("/api/v1/auth/register/confirm", {
        body,
      });
      if (error) {
        throw error;
      }
      return data as Account;
    },
    onSuccess: (account) => {
      queryClient.setQueryData(sessionQueryOptions.queryKey, account);
    },
  });
}

/**
 * Change your own password.
 *
 * On success the server signs out every other session and issues this browser
 * a new one; the response is the updated account (`must_change_password` now
 * `false`). The session cache is overwritten first so the route guard lets the
 * user leave the change-password page, then every other cache is invalidated:
 * they were all `403` while the change was forced, and the `sessions` list
 * changed because the others were signed out.
 */
export function useChangePassword() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (
      body: components["schemas"]["ChangePasswordRequest"]
    ) => {
      const { data, error } = await api.PATCH("/api/v1/auth/password", {
        body,
      });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: (account) => {
      queryClient.setQueryData(sessionQueryOptions.queryKey, account);
      const session = hashKey(sessionQueryOptions.queryKey);
      queryClient.invalidateQueries({
        predicate: (query) => query.queryHash !== session,
      });
    },
  });
}
