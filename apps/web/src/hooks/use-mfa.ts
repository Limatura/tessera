import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

type ConfirmOwnerRequest = components["schemas"]["ConfirmOwnerRequest"];

const MFA_KEY = ["account", "mfa"] as const;

/** Cookie only: a bearer token gets 403 on every `/account/mfa` endpoint. */
export function mfaStatusQueryOptions() {
  return queryOptions({
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/account/mfa");
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: MFA_KEY,
  });
}

export function useMfaStatus() {
  return useQuery(mfaStatusQueryOptions());
}

/**
 * Starts setting up an authenticator app. Nothing is on yet: the secret only
 * takes effect once `useConfirmTotp` proves the app has it.
 */
export function useBeginTotp() {
  return useMutation({
    mutationFn: async (currentPassword: string | undefined) => {
      const { data, error } = await api.POST("/api/v1/account/mfa/totp", {
        body: { current_password: currentPassword },
      });
      if (error) {
        throw error;
      }
      return data;
    },
  });
}

/** Turns it on. The answer is the recovery codes, shown this once only. */
export function useConfirmTotp() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (code: string) => {
      const { data, error } = await api.POST(
        "/api/v1/account/mfa/totp/confirm",
        { body: { code } }
      );
      if (error) {
        throw error;
      }
      return data.recovery_codes;
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: MFA_KEY }),
  });
}

export function useDisableTotp() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: ConfirmOwnerRequest) => {
      const { error } = await api.DELETE("/api/v1/account/mfa/totp", { body });
      if (error) {
        throw error;
      }
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: MFA_KEY }),
  });
}

/** Replaces every recovery code; the old ones stop working at once. */
export function useRegenerateRecoveryCodes() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: ConfirmOwnerRequest) => {
      const { data, error } = await api.POST(
        "/api/v1/account/mfa/recovery-codes",
        { body }
      );
      if (error) {
        throw error;
      }
      return data.recovery_codes;
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: MFA_KEY }),
  });
}
