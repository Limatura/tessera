import { api } from "@tessera/api";

import type { WebhooksSource } from "@/components/webhooks/types";

import { adminKeys } from "./keys";

/** The instance's webhooks: they hear about every share, anonymous ones included. */
export const instanceWebhooks: WebhooksSource = {
  create: async (body) => {
    const { data, error } = await api.POST("/api/v1/admin/webhooks", { body });
    if (error) {
      throw error;
    }
    return data;
  },
  deliveries: async (id) => {
    const { data, error } = await api.GET(
      "/api/v1/admin/webhooks/{id}/deliveries",
      { params: { path: { id } } }
    );
    if (error) {
      throw error;
    }
    return data;
  },
  key: adminKeys.webhooks(),
  list: async () => {
    const { data, error } = await api.GET("/api/v1/admin/webhooks");
    if (error) {
      throw error;
    }
    return data;
  },
  remove: async (id) => {
    const { error } = await api.DELETE("/api/v1/admin/webhooks/{id}", {
      params: { path: { id } },
    });
    if (error) {
      throw error;
    }
  },
};
