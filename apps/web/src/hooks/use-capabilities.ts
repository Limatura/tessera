import { queryOptions, useQuery } from "@tanstack/react-query";
import { api } from "@tessera/api";

export const capabilitiesQueryOptions = queryOptions({
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/capabilities", {});
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: ["capabilities"],
  retry: false,
  staleTime: 1000 * 60 * 5,
});

export function useCapabilities() {
  return useQuery(capabilitiesQueryOptions);
}
