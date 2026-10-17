// The one service worker for the whole origin. A scope holds a single worker,
// so everything that needs one lives here:
//
// 1. Streamed downloads, for browsers without `showSaveFilePicker` (Safari,
//    Firefox). See `src/lib/download/save-stream.ts`. The page posts `start`
//    with a port, then opens `/__download/{id}` in a hidden frame. This answers
//    that navigation with a stream that asks the page for each piece over the
//    port, so the browser's own download takes the bytes.
// 2. The Web Share Target. The manifest (served by the backend) points the
//    system share sheet at `POST /share-target`; the shared files and text are
//    put in IndexedDB and the page is opened at `/?share={id}` to pick them up.
//    See `src/lib/pwa/shared-payload.ts`.
// 3. A small cache: hashed `/assets/*` cache-first, and `/` as a fallback for
//    navigations while offline. `index.html` is never served from the cache
//    while the network answers, because the server writes the instance's name
//    and colours into it.
//
// Everything else goes to the network as if this worker did not exist.

const DOWNLOAD_PREFIX = "/__download/";
/** Downloads announced but not yet opened, by id. */
const pending = new Map();

const SHARE_ACTION = "/share-target";
// Must match `src/lib/pwa/shared-payload.ts`.
const SHARE_DB = "tessera-share";
const SHARE_STORE = "payloads";
/** A share nobody picked up (the tab was closed first) is dropped after this. */
const SHARE_TTL_MS = 24 * 60 * 60 * 1000;

const CACHE = "tessera-v1";
const SHELL = "/";
/** Old hashed chunks pile up across deployments; past this, the oldest go. */
const MAX_CACHED_ASSETS = 300;

self.addEventListener("install", (event) => {
  self.skipWaiting();
  // Best effort: an install that fails to fetch the shell still installs.
  event.waitUntil(
    (async () => {
      try {
        const cache = await caches.open(CACHE);
        await cache.add(SHELL);
      } catch {
        // The offline fallback stays empty until the next visit to `/`
      }
    })()
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const names = await caches.keys();
      await Promise.all(
        names
          .filter((name) => name.startsWith("tessera-") && name !== CACHE)
          .map((name) => caches.delete(name))
      );
      await self.clients.claim();
    })()
  );
});

self.addEventListener("message", (event) => {
  const message = event.data;
  // `ping` needs no answer: receiving an event is what keeps Firefox from
  // stopping this worker mid-download.
  if (message?.type !== "start") {
    return;
  }
  const [port] = event.ports;
  pending.set(message.id, { name: message.name, port, size: message.size });
  port.postMessage({ type: "ready" });
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) {
    return;
  }
  if (url.pathname.startsWith(DOWNLOAD_PREFIX)) {
    const id = url.pathname.slice(DOWNLOAD_PREFIX.length);
    const download = pending.get(id);
    if (!download) {
      event.respondWith(new Response("Not found", { status: 404 }));
      return;
    }
    pending.delete(id);
    event.respondWith(respond(download));
    return;
  }
  if (request.method === "POST" && url.pathname === SHARE_ACTION) {
    event.respondWith(receiveShare(request));
    return;
  }
  if (request.method !== "GET") {
    return;
  }
  if (url.pathname.startsWith("/assets/")) {
    event.respondWith(cacheFirst(request));
    return;
  }
  if (request.mode === "navigate" && isAppRoute(url.pathname)) {
    event.respondWith(networkFirst(request, url.pathname));
  }
});

// oxlint-disable-next-line eslint/no-implicit-globals -- a worker script, not a page
async function receiveShare(request) {
  try {
    const form = await request.formData();
    const text = (name) => {
      const value = form.get(name);
      return typeof value === "string" ? value : "";
    };
    // An empty file input still arrives as a nameless, empty File on some
    // browsers.
    const files = form
      .getAll("files")
      .filter((file) => typeof file !== "string" && file.name !== "");
    const id = crypto.randomUUID();
    await storeShare(id, {
      files,
      receivedAt: Date.now(),
      text: text("text"),
      title: text("title"),
      url: text("url"),
    });
    return Response.redirect(`/?share=${id}`, 303);
  } catch {
    // A share that cannot be read still opens the app, just empty-handed.
    return Response.redirect("/", 303);
  }
}

// oxlint-disable-next-line eslint/no-implicit-globals -- a worker script, not a page
function openShareDb() {
  // oxlint-disable-next-line promise/avoid-new -- IndexedDB only has events
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(SHARE_DB, 1);
    open.addEventListener("upgradeneeded", () => {
      open.result.createObjectStore(SHARE_STORE);
    });
    open.addEventListener("success", () => resolve(open.result));
    open.addEventListener("error", () => reject(open.error));
  });
}

// oxlint-disable-next-line eslint/no-implicit-globals -- a worker script, not a page
async function storeShare(id, payload) {
  const db = await openShareDb();
  try {
    // oxlint-disable-next-line promise/avoid-new -- IndexedDB only has events
    await new Promise((resolve, reject) => {
      const tx = db.transaction(SHARE_STORE, "readwrite");
      const store = tx.objectStore(SHARE_STORE);
      const cursor = store.openCursor();
      cursor.addEventListener("success", () => {
        const current = cursor.result;
        if (!current) {
          return;
        }
        if (Date.now() - current.value.receivedAt > SHARE_TTL_MS) {
          current.delete();
        }
        current.continue();
      });
      store.put(payload, id);
      tx.addEventListener("complete", resolve);
      tx.addEventListener("error", () => reject(tx.error));
      tx.addEventListener("abort", () => reject(tx.error));
    });
  } finally {
    db.close();
  }
}

/** A path the SPA renders, as opposed to a backend route a link points at. */
// oxlint-disable-next-line eslint/no-implicit-globals -- a worker script, not a page
function isAppRoute(pathname) {
  return (
    !pathname.startsWith("/api/") &&
    !pathname.startsWith("/docs") &&
    !/^\/s\/[^/]+\/./u.test(pathname)
  );
}

// oxlint-disable-next-line eslint/no-implicit-globals -- a worker script, not a page
async function cacheFirst(request) {
  const cache = await caches.open(CACHE);
  const hit = await cache.match(request);
  if (hit) {
    return hit;
  }
  const response = await fetch(request);
  if (response.ok) {
    await cache.put(request, response.clone());
    await trimAssets(cache);
  }
  return response;
}

// oxlint-disable-next-line eslint/no-implicit-globals -- a worker script, not a page
async function trimAssets(cache) {
  const keys = await cache.keys();
  const assets = keys.filter((key) =>
    new URL(key.url).pathname.startsWith("/assets/")
  );
  // `keys()` lists in insertion order, so the front is the oldest.
  const excess = assets.slice(
    0,
    Math.max(0, assets.length - MAX_CACHED_ASSETS)
  );
  await Promise.all(excess.map((key) => cache.delete(key)));
}

// oxlint-disable-next-line eslint/no-implicit-globals -- a worker script, not a page
async function networkFirst(request, pathname) {
  try {
    const response = await fetch(request);
    // Only `/` is kept: a share page has that share's preview spliced in, and
    // a redirect (a link share) is not a page at all.
    if (pathname === SHELL && response.ok && !response.redirected) {
      const cache = await caches.open(CACHE);
      await cache.put(SHELL, response.clone());
    }
    return response;
  } catch (error) {
    const cached = await caches.match(SHELL);
    if (cached) {
      return cached;
    }
    throw error;
  }
}

// `encodeURIComponent` leaves `'()*` as they are, and RFC 5987 does not allow
// them in `filename*`: left alone they cut the name short in some browsers.
// oxlint-disable-next-line eslint/no-implicit-globals -- a worker script, not a page
function rfc5987(name) {
  return encodeURIComponent(name).replaceAll(
    /['()*]/gu,
    (char) => `%${char.codePointAt(0).toString(16).toUpperCase()}`
  );
}

// oxlint-disable-next-line eslint/no-implicit-globals -- a worker script, not a page
function respond({ name, port, size }) {
  let waiting;

  port.addEventListener("message", (event) => {
    const message = event.data;
    const resolve = waiting;
    waiting = undefined;
    resolve?.(message);
  });
  port.start();

  const stream = new ReadableStream(
    {
      cancel() {
        port.postMessage({ type: "cancel" });
        port.close();
      },
      async pull(controller) {
        // oxlint-disable-next-line promise/avoid-new -- the answer arrives as an event
        const answer = new Promise((resolve) => {
          waiting = resolve;
        });
        port.postMessage({ type: "pull" });
        const message = await answer;
        if (message.type === "chunk") {
          controller.enqueue(message.chunk);
          return;
        }
        port.close();
        if (message.type === "end") {
          controller.close();
          return;
        }
        controller.error(new Error("the page stopped sending"));
      },
    },
    { highWaterMark: 4 }
  );

  return new Response(stream, {
    headers: {
      "Content-Disposition": `attachment; filename*=UTF-8''${rfc5987(name)}`,
      "Content-Length": String(size),
      "Content-Type": "application/octet-stream",
    },
  });
}
