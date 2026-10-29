import type { components } from "@tessera/api/schema";

export type Webhook = components["schemas"]["WebhookResponse"];
export type CreatedWebhook = components["schemas"]["CreatedWebhookResponse"];
export type CreateWebhookRequest =
  components["schemas"]["CreateWebhookRequest"];
export type Delivery = components["schemas"]["DeliveryResponse"];

/**
 * An account's webhooks and the instance's are the same thing under different
 * endpoints. The shared components take whichever source applies, so the two
 * pages cannot drift apart. Plain functions and a key, not hooks: hooks picked
 * off an object are not statically known, which the rules of hooks refuse.
 */
export interface WebhooksSource {
  /** Prefix of every cache entry for this source, so one invalidation covers the list and its deliveries. */
  key: readonly string[];
  list: () => Promise<Webhook[]>;
  create: (body: CreateWebhookRequest) => Promise<CreatedWebhook>;
  remove: (id: string) => Promise<void>;
  deliveries: (id: string) => Promise<Delivery[]>;
}
