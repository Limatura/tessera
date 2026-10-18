/**
 * Copies text for the background. Chrome's service worker has no clipboard,
 * and an offscreen document is never focused, which `navigator.clipboard`
 * requires, so this is the old textarea-and-`execCommand` way, which Chrome
 * documents for exactly this case.
 */
import { browser } from "#imports";
import { isMessage } from "@/lib/messages";

browser.runtime.onMessage.addListener((message: unknown) => {
  if (!isMessage(message) || message.type !== "offscreen-copy") {
    return;
  }
  const area = document.querySelector<HTMLTextAreaElement>("#clipboard");
  if (!area) {
    return;
  }
  area.value = message.text;
  area.select();
  // oxlint-disable-next-line typescript/no-deprecated -- the only clipboard an unfocused document has
  document.execCommand("copy");
  area.value = "";
});
