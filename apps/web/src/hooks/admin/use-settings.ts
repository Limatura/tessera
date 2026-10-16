import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

import { adminKeys } from "./keys";

type Settings = components["schemas"]["SettingsResponse"];
type Changes = components["schemas"]["UpdateSettingsRequest"]["changes"];

/**
 * Instance settings as a flat list of dotted keys (`share.code_length`,
 * `features.admin`...).
 *
 * Read-only entries are included too, with `editable: false` and a `refusal`
 * saying why. Leaving them out would only make people ask where the port
 * setting went.
 */
export const settingsQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/settings", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: adminKeys.settings(),
});

export function useSettings() {
  return useQuery(settingsQueryOptions);
}

/**
 * Reads one setting's value by dotted key.
 *
 * Returns `unknown` because OpenAPI describes `value` as opaque JSON whose type
 * depends on the key, so the caller compares it. A missing key (still loading,
 * or a server version without it) is also `undefined`, so compare against a
 * concrete value instead of using it as a boolean.
 */
export function readSetting(
  settings: Settings | undefined,
  key: string
): unknown {
  return settings?.settings.find((setting) => setting.key === key)?.value;
}

/**
 * Changes settings. A batch is validated as one, and one invalid value rejects
 * all of it, so only fields meant to take effect together belong in one call.
 *
 * A change that deletes something (a shorter retention period) is refused with
 * `confirmation_required` unless `confirm` is set; set it only after showing
 * what `usePreviewChanges` says it removes.
 */
export function useUpdateSettings() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      changes,
      confirm = false,
    }: {
      changes: Record<string, unknown>;
      confirm?: boolean;
    }) => {
      const { data, error } = await api.PUT("/api/v1/admin/settings", {
        // openapi-typescript renders "any JSON value" as `Record<string, never>`.
        // The cast works around that type, not the validation: the server checks
        // each key's value and rejects the batch if one fails.
        body: { changes: changes as Changes, confirm },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: settingsQueryOptions.queryKey,
      });
    },
  });
}

type Consequence = components["schemas"]["Consequence"];

/** `{key: null}` for each key: clearing is how a setting goes back to its fallback. */
export function clearing(keys: readonly string[]): Changes {
  return Object.fromEntries(keys.map((key) => [key, null])) as Changes;
}

/**
 * What a batch of changes would delete on the next sweep (a retention period
 * made shorter), without applying it. Empty when nothing. The same validation
 * as `PUT`, so a clean preview is not refused after.
 */
export function usePreviewChanges() {
  return useMutation({
    mutationFn: async (
      changes: Record<string, unknown>
    ): Promise<Consequence[]> => {
      const { data, error } = await api.POST("/api/v1/admin/settings/preview", {
        body: { changes: changes as Changes },
      });
      if (error) {
        throw error;
      }
      return data.consequences;
    },
  });
}

/**
 * Clears the panel's value for these keys, so each goes back to the
 * environment's or the built-in default. Sent with `confirm`: the caller has
 * shown `usePreviewChanges`'s consequences for `clearing(keys)` and the admin went ahead.
 */
export function useResetSettings() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (keys: readonly string[]) => {
      const { data, error } = await api.PUT("/api/v1/admin/settings", {
        body: { changes: clearing(keys), confirm: true },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: settingsQueryOptions.queryKey,
      });
    },
  });
}
