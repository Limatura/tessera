/**
 * Whether the site can be installed, and how.
 *
 * - `prompt`: Chromium fired `beforeinstallprompt`; its `prompt()` opens the
 *   browser's own install dialog. The event fires once, often before React
 *   mounts, so it is caught at import time (main.tsx imports this early) and
 *   kept here.
 * - `ios`: iOS has no install API at all; the only way is the share sheet's
 *   "Add to Home Screen", so the button explains that instead.
 * - `null`: already installed (running standalone), or nothing to offer.
 */
export type InstallMode = "prompt" | "ios" | null;

/** Chromium's event, which TypeScript's DOM library does not describe. */
interface BeforeInstallPromptEvent extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

let deferred: BeforeInstallPromptEvent | null = null;
let installed = false;
const listeners = new Set<() => void>();

function notify() {
  for (const listener of listeners) {
    listener();
  }
}

if (typeof window !== "undefined") {
  window.addEventListener("beforeinstallprompt", (event) => {
    // Keeps the browser's mini-infobar away; the header button replaces it
    event.preventDefault();
    deferred = event as BeforeInstallPromptEvent;
    notify();
  });
  window.addEventListener("appinstalled", () => {
    deferred = null;
    installed = true;
    notify();
  });
}

function isStandalone(): boolean {
  return (
    window.matchMedia("(display-mode: standalone)").matches ||
    (navigator as Navigator & { standalone?: boolean }).standalone === true
  );
}

function isIos(): boolean {
  // iPadOS reports itself as a Mac; the touch points give it away
  return (
    /\b(?:iPhone|iPad|iPod)\b/u.test(navigator.userAgent) ||
    (/\bMacintosh\b/u.test(navigator.userAgent) && navigator.maxTouchPoints > 1)
  );
}

export function getInstallMode(): InstallMode {
  if (typeof window === "undefined" || installed || isStandalone()) {
    return null;
  }
  if (deferred) {
    return "prompt";
  }
  return isIos() ? "ios" : null;
}

export function subscribeToInstallMode(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Opens the browser's install dialog. Only meaningful in `prompt` mode. */
export async function promptInstall(): Promise<void> {
  const event = deferred;
  if (!event) {
    return;
  }
  // The event can be prompted only once, whatever the answer
  deferred = null;
  notify();
  await event.prompt();
  await event.userChoice;
}
