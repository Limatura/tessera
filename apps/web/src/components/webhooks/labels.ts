import * as m from "@/paraglide/messages";

export const WEBHOOK_EVENTS = [
  "share.created",
  "share.downloaded",
  "share.expired",
] as const;

/** An event a newer server adds shows as its code until it is translated. */
export function eventLabel(event: string): string {
  switch (event) {
    case "share.created": {
      return m.webhooks_event_created();
    }
    case "share.downloaded": {
      return m.webhooks_event_downloaded();
    }
    case "share.expired": {
      return m.webhooks_event_expired();
    }
    default: {
      return event;
    }
  }
}

export function deliveryStateLabel(state: string): string {
  switch (state) {
    case "delivered": {
      return m.webhooks_state_delivered();
    }
    case "failed": {
      return m.webhooks_state_failed();
    }
    case "pending": {
      return m.webhooks_state_pending();
    }
    default: {
      return state;
    }
  }
}
