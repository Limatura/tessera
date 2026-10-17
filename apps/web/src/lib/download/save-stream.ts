/**
 * Writes a stream the page produces to a file the visitor keeps, without
 * holding it in memory. See `docs/retrieval.md`, "Joining the pieces in the
 * browser".
 *
 * - Chromium: `showSaveFilePicker()` and a writable straight into that file.
 * - Safari and Firefox: a service worker answers a navigation with the stream,
 *   so the browser's own download takes it.
 *
 * Firefox stops an idle service worker after 30 seconds and its download then
 * looks finished while holding only part of the file, so the page pings the
 * worker every 5 seconds for as long as it is feeding it. That is also why the
 * tab must stay open: the ping comes from here.
 */

import { registerServiceWorker } from "@/lib/pwa/service-worker";

/** Chromium's picker, which TypeScript's DOM library does not describe yet. */
interface SavePickerWindow {
  showSaveFilePicker?: (options: {
    suggestedName?: string;
  }) => Promise<{ createWritable: () => Promise<WritableStream<Uint8Array>> }>;
}

/** Where the stream goes, decided before anything is fetched. */
export interface SaveTarget {
  /** Takes the bytes; resolves once all of them are written. */
  write: (stream: ReadableStream<Uint8Array>) => Promise<void>;
  /** Throws the target away, for when the stream never starts. */
  cancel: () => void | Promise<void>;
}

const WORKER_PREFIX = "/__download/";
const PING_MS = 5000;
const FRAME_LINGER_MS = 60_000;

export function canSaveStreams(): boolean {
  return (
    typeof window !== "undefined" &&
    (typeof (window as SavePickerWindow).showSaveFilePicker === "function" ||
      "serviceWorker" in navigator)
  );
}

/**
 * Picks where the file goes. Call it straight from a click: the picker needs
 * the user's gesture, which an `await` before it would lose.
 *
 * Resolves to `null` when the visitor dismisses the picker.
 */
export async function openSaveTarget(
  name: string,
  size: number
): Promise<SaveTarget | null> {
  const picker = (window as SavePickerWindow).showSaveFilePicker;
  if (typeof picker === "function") {
    try {
      const handle = await picker.call(window, { suggestedName: name });
      const writable = await handle.createWritable();
      return pickerTarget(writable, size);
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") {
        return null;
      }
      throw error;
    }
  }
  return workerTarget(name, size);
}

function pickerTarget(
  writable: WritableStream<Uint8Array>,
  size: number
): SaveTarget {
  return {
    cancel: async () => {
      try {
        await writable.abort();
      } catch {
        // Already closed: nothing left to throw away
      }
    },
    write: async (stream) => {
      let written = 0;
      const counted = stream.pipeThrough(
        new TransformStream<Uint8Array, Uint8Array>({
          flush() {
            // Checked before the file is closed, so a short one is discarded
            if (written !== size) {
              throw new Error(`wrote ${written} of ${size} bytes`);
            }
          },
          transform(chunk, controller) {
            written += chunk.length;
            controller.enqueue(chunk);
          },
        })
      );
      await guardUnload(() => counted.pipeTo(writable));
    },
  };
}

async function workerTarget(name: string, size: number): Promise<SaveTarget> {
  const worker = await activeWorker();
  const id = crypto.randomUUID();
  return {
    cancel: () => {
      // Nothing was handed to the worker yet
    },
    write: (stream) =>
      guardUnload(() => feedWorker(worker, id, name, size, stream)),
  };
}

async function activeWorker(): Promise<ServiceWorker> {
  const registration = await registerServiceWorker();
  await navigator.serviceWorker.ready;
  const worker =
    registration.active ?? registration.waiting ?? registration.installing;
  if (!worker) {
    throw new Error("the download worker did not start");
  }
  if (worker.state !== "activated") {
    // oxlint-disable-next-line promise/avoid-new -- statechange is only an event
    await new Promise<void>((resolve) => {
      const onChange = () => {
        if (worker.state === "activated") {
          worker.removeEventListener("statechange", onChange);
          resolve();
        }
      };
      worker.addEventListener("statechange", onChange);
    });
  }
  return worker;
}

/**
 * The worker asks for each piece when its stream wants more, so a slow disk
 * holds the network back instead of piling bytes up in either context.
 * Streams are not transferable in Safari, hence pieces over a port.
 */
function feedWorker(
  worker: ServiceWorker,
  id: string,
  name: string,
  size: number,
  stream: ReadableStream<Uint8Array>
): Promise<void> {
  const reader = stream.getReader();
  const channel = new MessageChannel();
  const ping = setInterval(
    // oxlint-disable-next-line unicorn/require-post-message-target-origin -- a worker's postMessage takes no origin
    () => worker.postMessage({ type: "ping" }),
    PING_MS
  );
  let frame: HTMLIFrameElement | undefined;

  const cleanup = () => {
    clearInterval(ping);
    channel.port1.close();
    // The browser's download outlives the frame that started it, but removing
    // it at once makes some engines cancel: give the download time to settle.
    const started = frame;
    setTimeout(() => started?.remove(), FRAME_LINGER_MS);
  };

  // oxlint-disable-next-line promise/avoid-new -- the port speaks in events
  return new Promise<void>((resolve, reject) => {
    channel.port1.addEventListener("message", async (event: MessageEvent) => {
      const message = event.data as { type: string };
      if (message.type === "ready") {
        // A navigation the worker answers with an attachment: the browser
        // saves it and the page stays where it is.
        frame = document.createElement("iframe");
        frame.hidden = true;
        frame.src = `${WORKER_PREFIX}${id}`;
        document.body.append(frame);
        return;
      }
      if (message.type === "cancel") {
        cleanup();
        try {
          await reader.cancel();
        } catch {
          // Already failed: that error is the one reported
        }
        reject(new DOMException("Download cancelled", "AbortError"));
        return;
      }
      if (message.type !== "pull") {
        return;
      }
      try {
        const { done, value } = await reader.read();
        if (done) {
          channel.port1.postMessage({ type: "end" });
          cleanup();
          resolve();
          return;
        }
        // Transfer, not copy, but only a buffer the piece owns whole: a view
        // into a larger one would hand the rest of it over too.
        const owned =
          value.byteOffset === 0 && value.byteLength === value.buffer.byteLength
            ? value
            : new Uint8Array(value);
        channel.port1.postMessage({ chunk: owned, type: "chunk" }, [
          owned.buffer,
        ]);
      } catch (error) {
        channel.port1.postMessage({ type: "error" });
        cleanup();
        reject(error);
      }
    });
    channel.port1.start();
    // oxlint-disable-next-line unicorn/require-post-message-target-origin -- a worker's postMessage takes no origin
    worker.postMessage({ id, name, size, type: "start" }, [channel.port2]);
  });
}

/** Closing the tab mid-transfer cuts the file short, so ask first. */
async function guardUnload<T>(run: () => Promise<T>): Promise<T> {
  window.addEventListener("beforeunload", warn);
  try {
    return await run();
  } finally {
    window.removeEventListener("beforeunload", warn);
  }
}

function warn(event: BeforeUnloadEvent) {
  event.preventDefault();
}
