import type { components } from "@tessera/api/schema";

type ShareResponse = components["schemas"]["ShareResponse"];

/** First wait before reconnecting a dropped stream; doubles up to the cap. */
const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 15_000;

interface WatchOptions {
  code: string;
  /** An anonymous upload's token; a signed-in owner authenticates by cookie. */
  managementToken?: string;
  onShare: (share: ShareResponse) => void;
}

/**
 * Follows `GET /shares/{code}/manage/events` and reports each `share` event.
 * `EventSource` cannot send `X-Share-Management-Token`, and the token must not
 * go in the URL, so the stream is read with `fetch`.
 *
 * The server closes the stream once nothing waits on the scanner; that ends the
 * watch. A stream that drops before then (restart, network) is reopened with
 * backoff. A refusal such as `404` ends it, since retrying cannot help.
 *
 * Returns a function that stops watching.
 */
export function watchManagedShare({
  code,
  managementToken,
  onShare,
}: WatchOptions): () => void {
  const controller = new AbortController();

  const run = async () => {
    let delay = RECONNECT_BASE_MS;
    while (!controller.signal.aborted) {
      let settled = false;
      try {
        // oxlint-disable-next-line no-await-in-loop -- reconnects are sequential
        const response = await fetch(
          `/api/v1/shares/${encodeURIComponent(code)}/manage/events`,
          {
            credentials: "include",
            headers: {
              Accept: "text/event-stream",
              ...(managementToken
                ? { "X-Share-Management-Token": managementToken }
                : {}),
            },
            signal: controller.signal,
          }
        );
        if (!response.ok || !response.body) {
          if (response.status >= 400 && response.status < 500) {
            return;
          }
          throw new Error(response.statusText);
        }
        // oxlint-disable-next-line no-await-in-loop -- events arrive in order
        for await (const data of readShareEvents(response.body)) {
          delay = RECONNECT_BASE_MS;
          const share = JSON.parse(data) as ShareResponse;
          settled = !share.scanning;
          onShare(share);
        }
      } catch {
        if (controller.signal.aborted) {
          return;
        }
      }
      if (settled) {
        return;
      }
      // oxlint-disable-next-line no-await-in-loop -- backoff before the next attempt
      await sleep(delay, controller.signal);
      delay = Math.min(delay * 2, RECONNECT_MAX_MS);
    }
  };

  void run();
  return () => controller.abort();
}

/**
 * @yields {string} The `data` of each `share` event; comment lines (keep-alives) are skipped.
 */
async function* readShareEvents(
  body: ReadableStream<Uint8Array>
): AsyncGenerator<string> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  try {
    for (;;) {
      // oxlint-disable-next-line no-await-in-loop -- one read per chunk
      const { done, value } = await reader.read();
      if (done) {
        return;
      }
      buffer += decoder.decode(value, { stream: true });
      let end = buffer.search(/\r?\n\r?\n/u);
      while (end !== -1) {
        const block = buffer.slice(0, end);
        buffer = buffer.slice(end).replace(/^\r?\n\r?\n/u, "");
        const data = parseBlock(block);
        if (data !== null) {
          yield data;
        }
        end = buffer.search(/\r?\n\r?\n/u);
      }
    }
  } finally {
    try {
      await reader.cancel();
    } catch {
      // The connection is already gone.
    }
  }
}

function parseBlock(block: string): string | null {
  let event = "message";
  const data: string[] = [];
  for (const line of block.split(/\r?\n/u)) {
    if (line.startsWith(":")) {
      continue;
    }
    const colon = line.indexOf(":");
    const field = colon === -1 ? line : line.slice(0, colon);
    const value = colon === -1 ? "" : line.slice(colon + 1).replace(/^ /u, "");
    if (field === "event") {
      event = value;
    } else if (field === "data") {
      data.push(value);
    }
  }
  return event === "share" && data.length > 0 ? data.join("\n") : null;
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
  // oxlint-disable-next-line promise/avoid-new -- wraps a timer
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, ms);
    signal.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        resolve();
      },
      { once: true }
    );
  });
}
