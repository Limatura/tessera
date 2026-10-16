import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";

import type { Appearance } from "@/lib/appearance";
import { parseAppearance } from "@/lib/appearance";

const PREFERENCES_KEY = ["preferences"] as const;

/**
 * The appearance this user chose. Signed-out visitors never get here: the
 * preferences page is behind `requireAuth`, and anonymous visitors get the site
 * default. `appearance` is an open object in the OpenAPI document (utoipa emits
 * `Object`), so it is parsed into three enums here instead of carrying
 * `Record<string, never>` around.
 */
export function preferencesQueryOptions() {
  return queryOptions({
    queryFn: async (): Promise<Appearance> => {
      const { data, error } = await api.GET("/api/v1/me/preferences");
      if (error) {
        throw error;
      }
      return parseAppearance(data.appearance);
    },
    queryKey: PREFERENCES_KEY,
    // Colours do not change by themselves; refetching on focus would only cause flicker.
    refetchOnWindowFocus: false,
  });
}

/**
 * @param enabled Pass `false` when signed out. The endpoint would answer 401,
 * which here is not an error to retry or show but "this user has no
 * preferences", so the request is not made at all.
 */
export function usePreferences(enabled = true) {
  return useQuery({ ...preferencesQueryOptions(), enabled });
}

/**
 * Replaces the whole value and returns the new one. The optimistic update is
 * needed: without it the user would see their chosen colour snap back before
 * the round trip returns and then change again. On failure the old value is
 * restored and the caller shows the error.
 *
 * It sends the whole `Appearance`, not just the changed field: the endpoint is a
 * PUT, and omitted fields are cleared as "no opinion".
 */
export function useUpdatePreferences() {
  const queryClient = useQueryClient();

  return useMutation<Appearance, Error, Appearance, { previous?: Appearance }>({
    mutationFn: async (appearance) => {
      const { data, error } = await api.PUT("/api/v1/me/preferences", {
        // utoipa emits `Appearance` as an open `Object`, which openapi-typescript
        // turns into `Record<string, never>` (a type no field can be put in). The
        // real contract is three optional enums, so the type is cut at this
        // boundary; the value already is an `Appearance`.
        body: { appearance: appearance as Record<string, never> },
      });
      if (error) {
        throw error;
      }
      return parseAppearance(data.appearance);
    },
    onError: (_error, _appearance, context) => {
      queryClient.setQueryData(PREFERENCES_KEY, context?.previous);
    },
    onMutate: async (appearance) => {
      await queryClient.cancelQueries({ queryKey: PREFERENCES_KEY });
      const previous = queryClient.getQueryData<Appearance>(PREFERENCES_KEY);
      queryClient.setQueryData(PREFERENCES_KEY, appearance);
      return { previous };
    },
    onSuccess: (saved) => {
      queryClient.setQueryData(PREFERENCES_KEY, saved);
    },
  });
}
