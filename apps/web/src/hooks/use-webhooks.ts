import { api } from "@tessera/api";

import type { WebhooksSource } from "@/components/webhooks/types";

/** An account's own webhooks. Adding and removing are cookie only, so a token cannot. */
export const accountWebhooks: WebhooksSource = {
  create: async (body) => {
    const { data, error } = await api.POST("/api/v1/webhooks", { body });
    if (error) {
      throw error;
    }
    return data;
  },
  deliveries: async (id) => {
    const { data, error } = await api.GET("/api/v1/webhooks/{id}/deliveries", {
      params: { path: { id } },
    });
    if (error) {
      throw error;
    }
    return data;
  },
  key: ["webhooks"],
  list: async () => {
    const { data, error } = await api.GET("/api/v1/webhooks");
    if (error) {
      throw error;
    }
    return data;
  },
  remove: async (id) => {
    const { error } = await api.DELETE("/api/v1/webhooks/{id}", {
      params: { path: { id } },
    });
    if (error) {
      throw error;
    }
  },
};
