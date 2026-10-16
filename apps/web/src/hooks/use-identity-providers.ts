import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

export type Provider = components["schemas"]["ProviderResponse"];
export type Identity = components["schemas"]["IdentityResponse"];

const PROVIDERS_KEY = ["auth", "providers"] as const;
const IDENTITIES_KEY = ["auth", "identities"] as const;

/**
 * All three providers are returned, disabled ones too: the sign-in page has to
 * ask before it knows how many buttons to draw. Account settings do not mention
 * a disabled provider at all, so both sides filter on `enabled` themselves.
 *
 * With accounts turned off entirely this answers 404 and `error` is an
 * ErrorBody. Such an instance has no sign-in page anyway, so any error is
 * treated as "no third-party sign-in" (see providerList).
 */
export const providersQueryOptions = queryOptions({
  queryFn: async (): Promise<Provider[]> => {
    const { data, error } = await api.GET("/api/v1/auth/providers");
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: PROVIDERS_KEY,
  retry: false,
  // Instance configuration, not user data; it does not change during one page load.
  staleTime: 1000 * 60 * 5,
});

export function useProviders() {
  return useQuery(providersQueryOptions);
}

/**
 * Only configured providers can be drawn as buttons. A failed query is an empty
 * list and password sign-in still works.
 */
export function useEnabledProviders(): Provider[] {
  const { data } = useProviders();
  return data?.filter((provider) => provider.enabled) ?? [];
}

export function useIdentities() {
  return useQuery({
    queryFn: async (): Promise<Identity[]> => {
      const { data, error } = await api.GET("/api/v1/auth/oidc/identities");
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: IDENTITIES_KEY,
  });
}

/**
 * Unlinks a provider. 409 (`last_sign_in_method`) means it is the last way to
 * sign in; the server works that out, since it knows whether there is a
 * password and the frontend does not.
 */
export function useUnlinkIdentity() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (provider: string) => {
      const { error } = await api.DELETE(
        "/api/v1/auth/oidc/identities/{provider}",
        { params: { path: { provider } } }
      );
      if (error) {
        throw error;
      }
    },
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: IDENTITIES_KEY }),
  });
}
