import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import type { CreateWebhookRequest, WebhooksSource } from "./types";

export function useWebhookList(source: WebhooksSource) {
  return useQuery({ queryFn: source.list, queryKey: source.key });
}

/** The response carries `secret`, which the server never returns again. */
export function useCreateWebhook(source: WebhooksSource) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateWebhookRequest) => source.create(body),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: source.key }),
  });
}

export function useRemoveWebhook(source: WebhooksSource) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => source.remove(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: source.key }),
  });
}

/** `enabled` is false while no webhook is open, so nothing is fetched for a closed dialog. */
export function useWebhookDeliveries(
  source: WebhooksSource,
  id: string,
  enabled: boolean
) {
  return useQuery({
    enabled,
    queryFn: () => source.deliveries(id),
    queryKey: [...source.key, id, "deliveries"],
  });
}
