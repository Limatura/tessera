// Saves a file the page streams to it, for browsers without
// `showSaveFilePicker` (Safari, Firefox). See `src/lib/download/save-stream.ts`.
//
// The page posts `start` with a port, then opens `/__download/{id}` in a hidden
// frame. This answers that navigation with a stream that asks the page for
// each piece over the port, so the browser's own download takes the bytes.
//
// It touches no other request: everything else goes to the network as if this
// worker did not exist.

const PREFIX = "/__download/";
/** Downloads announced but not yet opened, by id. */
const pending = new Map();

self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
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
  const url = new URL(event.request.url);
  if (url.origin !== self.location.origin || !url.pathname.startsWith(PREFIX)) {
    return;
  }
  const id = url.pathname.slice(PREFIX.length);
  const download = pending.get(id);
  if (!download) {
    event.respondWith(new Response("Not found", { status: 404 }));
    return;
  }
  pending.delete(id);
  event.respondWith(respond(download));
});

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
