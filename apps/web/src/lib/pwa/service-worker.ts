/**
 * The origin's one service worker (`public/sw.js`): streamed downloads, the
 * share target and the asset cache all live in it, because a scope holds a
 * single worker. Every caller goes through here so nothing registers a second
 * script over it.
 */
const WORKER_PATH = "/sw.js";

let registration: Promise<ServiceWorkerRegistration> | undefined;

export function registerServiceWorker(): Promise<ServiceWorkerRegistration> {
  registration ??= navigator.serviceWorker.register(WORKER_PATH, {
    scope: "/",
  });
  return registration;
}

/**
 * Registers once the page has loaded, so the worker's install (which fetches
 * `/` for the offline fallback) does not compete with first paint.
 */
export function registerServiceWorkerWhenIdle() {
  if (!("serviceWorker" in navigator)) {
    return;
  }
  if (document.readyState === "complete") {
    registerQuietly();
  } else {
    window.addEventListener("load", registerQuietly, { once: true });
  }
}

async function registerQuietly() {
  try {
    await registerServiceWorker();
  } catch {
    // Without a worker the site still works; only installing it does not
  }
}
