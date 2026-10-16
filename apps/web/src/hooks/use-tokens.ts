import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

export type ApiToken = components["schemas"]["TokenResponse"];
type CreateTokenRequest = components["schemas"]["CreateTokenRequest"];

const TOKENS_KEY = ["tokens"] as const;

/** Listing accepts a bearer token too; creating is cookie only, so a token cannot mint another. */
export function tokensQueryOptions() {
  return queryOptions({
    queryFn: async (): Promise<ApiToken[]> => {
      const { data, error } = await api.GET("/api/v1/tokens");
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: TOKENS_KEY,
  });
}

export function useTokens() {
  return useQuery(tokensQueryOptions());
}

/** The response carries `secret`, which the server never returns again. */
export function useCreateToken() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: CreateTokenRequest) => {
      const { data, error } = await api.POST("/api/v1/tokens", { body });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: TOKENS_KEY }),
  });
}

export function useRevokeToken() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const { error } = await api.DELETE("/api/v1/tokens/{id}", {
        params: { path: { id } },
      });
      if (error) {
        throw error;
      }
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: TOKENS_KEY }),
  });
}
