import { browser } from "#imports";

/** What the background sends the offscreen document to have it copy. */
export interface OffscreenCopyMessage {
  type: "offscreen-copy";
  text: string;
}

const OFFSCREEN_PATH = "/offscreen.html";

/**
 * Puts `text` on the clipboard from wherever this runs.
 *
 * A page and Firefox's background page have `navigator.clipboard`. Chrome's
 * service worker has no clipboard and no document, so it borrows an
 * offscreen document, which exists for exactly this.
 */
export async function copyText(text: string): Promise<void> {
  if (typeof navigator !== "undefined" && navigator.clipboard) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // An unfocused page is refused; the offscreen document is not.
    }
  }
  if (!browser.offscreen) {
    throw new Error("no clipboard here");
  }
  await ensureOffscreen();
  await browser.runtime.sendMessage({
    text,
    type: "offscreen-copy",
  } satisfies OffscreenCopyMessage);
}

async function ensureOffscreen(): Promise<void> {
  const url = browser.runtime.getURL(OFFSCREEN_PATH);
  const contexts = await browser.runtime.getContexts({
    contextTypes: ["OFFSCREEN_DOCUMENT"],
    documentUrls: [url],
  });
  if (contexts.length > 0) {
    return;
  }
  await browser.offscreen.createDocument({
    justification: "Copy the link of a finished upload to the clipboard",
    reasons: ["CLIPBOARD"],
    url,
  });
}
