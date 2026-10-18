import { browser } from "#imports";

import type { OffscreenCopyMessage } from "./clipboard";

/** Every message an extension page or the background sends. */
export type Message =
  | { type: "upload-note"; text: string; title?: string }
  | { type: "cancel-upload"; id: string }
  | OffscreenCopyMessage;

export function send(message: Message): Promise<unknown> {
  return browser.runtime.sendMessage(message);
}

export function isMessage(value: unknown): value is Message {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { type?: unknown }).type === "string"
  );
}
