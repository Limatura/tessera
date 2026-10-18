/**
 * Server state for the extension's pages, through TanStack Query. Every key
 * starts with the instance and the token, so switching either one never
 * shows what the other could see.
 */
import { useQuery } from "@tanstack/react-query";

import { apiClient, errorMessage, fetchCapabilities } from "@/lib/instance";
import type { Account } from "@/lib/instance";
import { credentialItem, serverItem } from "@/lib/storage";
import type { Credential } from "@/lib/storage";

import { useStorageItem } from "./use-storage";

export interface Connection {
  /** `undefined` while loading, `null` when not configured. */
  server: string | null | undefined;
  /** Only a credential issued by `server`. */
  credential: Credential | null;
}

export function useConnection(): Connection {
  const server = useStorageItem(serverItem);
  const stored = useStorageItem(credentialItem);
  const credential =
    stored && server && stored.server === server ? stored : null;
  return { credential, server };
}

export function instanceKey({ server, credential }: Connection) {
  return ["instance", server, credential?.token ?? null] as const;
}

export function useCapabilities(connection: Connection) {
  const { server, credential } = connection;
  return useQuery({
    enabled: Boolean(server),
    queryFn: () => fetchCapabilities(server as string, credential),
    queryKey: [...instanceKey(connection), "capabilities"],
  });
}

/**
 * Who the token signs in as. `null` when the instance no longer accepts it
 * (revoked on the site, expired), which the popup offers to fix.
 */
export function useAccount(connection: Connection) {
  const { server, credential } = connection;
  return useQuery({
    enabled: Boolean(server && credential),
    queryFn: async (): Promise<Account | null> => {
      const { data, error, response } = await apiClient(
        server as string,
        credential
      ).GET("/api/v1/auth/session");
      if (response.status === 401) {
        return null;
      }
      if (error || !data) {
        throw new Error(errorMessage(error) ?? `session: ${response.status}`);
      }
      return data;
    },
    queryKey: [...instanceKey(connection), "account"],
  });
}

/** How many of the account's shares the popup lists. */
const MY_SHARES_LIMIT = 30;

export function useMyShares(connection: Connection) {
  const { server, credential } = connection;
  return useQuery({
    enabled: Boolean(server && credential),
    queryFn: async () => {
      const { data, error, response } = await apiClient(
        server as string,
        credential
      ).GET("/api/v1/shares", {
        params: { query: { limit: MY_SHARES_LIMIT } },
      });
      if (error || !data) {
        throw new Error(errorMessage(error) ?? `shares: ${response.status}`);
      }
      return data.data;
    },
    queryKey: [...instanceKey(connection), "shares"],
  });
}
