import { useMutation, useQuery } from "@tanstack/react-query";
import { api } from "@tessera/api";

/**
 * A program waiting to be approved, by the code it showed (see
 * apps/server/src/api/device.rs). A 404 means no request is waiting under that
 * code: mistyped, expired, or already answered. Not retried, since every miss
 * counts against this address's lookup budget.
 */
export function useDeviceAuthorization(code: string | undefined) {
  return useQuery({
    enabled: code !== undefined,
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/auth/device/{user_code}", {
        params: { path: { user_code: code ?? "" } },
      });
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: ["device-authorization", code],
    retry: false,
  });
}

/** Approves or denies the waiting request. */
export function useAnswerDeviceAuthorization() {
  return useMutation({
    mutationFn: async ({
      approve,
      code,
    }: {
      approve: boolean;
      code: string;
    }) => {
      const params = { path: { user_code: code } };
      const { error } = approve
        ? await api.POST("/api/v1/auth/device/{user_code}/approve", { params })
        : await api.POST("/api/v1/auth/device/{user_code}/deny", { params });
      if (error) {
        throw error;
      }
      return approve;
    },
  });
}
