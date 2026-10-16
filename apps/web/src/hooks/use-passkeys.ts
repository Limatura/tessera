import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

import { createPasskey } from "@/lib/webauthn";

export type Passkey = components["schemas"]["PasskeyResponse"];

const PASSKEYS_KEY = ["passkeys"] as const;

/** Cookie only, like sessions: a bearer token gets 403. */
export function passkeysQueryOptions() {
  return queryOptions({
    queryFn: async (): Promise<Passkey[]> => {
      const { data, error } = await api.GET("/api/v1/account/passkeys");
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: PASSKEYS_KEY,
  });
}

export function usePasskeys() {
  return useQuery(passkeysQueryOptions());
}

/**
 * The whole ceremony in one mutation: confirm the owner and get options, let
 * the browser make the key, then store it. A cancelled browser prompt rejects
 * with `NotAllowedError`; see `isPasskeyCancelled`.
 */
export function useAddPasskey() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      name,
      currentPassword,
    }: {
      name: string;
      currentPassword: string | undefined;
    }) => {
      const credential = await createPasskey(currentPassword);
      const { data, error } = await api.POST("/api/v1/account/passkeys", {
        body: { credential, name: name || undefined },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: PASSKEYS_KEY }),
  });
}

export function useRenamePasskey() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, name }: { id: string; name: string }) => {
      const { error } = await api.PATCH("/api/v1/account/passkeys/{id}", {
        body: { name },
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: PASSKEYS_KEY }),
  });
}

/** 409 when it is the account's last way in; the server decides. */
export function useRemovePasskey() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const { error } = await api.DELETE("/api/v1/account/passkeys/{id}", {
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: PASSKEYS_KEY }),
  });
}
