/**
 * Measuring the line to this instance from the browser.
 *
 * The server only moves bytes (`/api/v1/speed/*`); every clock reading here is
 * the browser's. Each direction is one test against `auth.throttle.speed`
 * (5 an hour per source by default), so a run spends two and latency is taken
 * from `/capabilities`, which is not counted.
 */

const DOWNLOAD_URL = "/api/v1/speed/download";
const UPLOAD_URL = "/api/v1/speed/upload";
const PING_URL = "/api/v1/capabilities";

/** How many round trips the latency is the median of. */
const PING_SAMPLES = 5;
/** `crypto.getRandomValues` refuses more than this per call. */
const RANDOM_CHUNK = 65_536;

/** A failed request, carrying the error body for `apiErrorMessage`. */
export class SpeedTestError extends Error {
  override readonly name = "SpeedTestError";
  readonly body: unknown;

  constructor(body: unknown) {
    super("speed test request failed");
    this.body = body;
  }
}

/** Progress of one direction: bytes so far and the rate over the transfer so far. */
export interface TransferProgress {
  bytes: number;
  mbps: number;
}

export interface TransferResult {
  bytes: number;
  seconds: number;
  mbps: number;
}

/** Megabits per second, the unit lines are sold in (10^6, not 2^20). */
export function toMbps(bytes: number, seconds: number): number {
  if (seconds <= 0) {
    return 0;
  }
  return (bytes * 8) / seconds / 1_000_000;
}

export function median(values: number[]): number {
  if (values.length === 0) {
    return 0;
  }
  const sorted = [...values].toSorted((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? ((sorted[middle - 1] ?? 0) + (sorted[middle] ?? 0)) / 2
    : (sorted[middle] ?? 0);
}

async function errorBody(response: Response): Promise<unknown> {
  try {
    return await response.json();
  } catch {
    return { code: "internal" };
  }
}

/**
 * Round-trip latency in milliseconds: the median of a few small requests, and
 * the spread between them (jitter, mean absolute difference of neighbours).
 */
export async function measureLatency(
  signal: AbortSignal
): Promise<{ latencyMs: number; jitterMs: number }> {
  const samples: number[] = [];
  for (let index = 0; index < PING_SAMPLES; index += 1) {
    const started = performance.now();
    // oxlint-disable-next-line no-await-in-loop -- samples must not overlap, or they measure each other
    const response = await fetch(PING_URL, {
      cache: "no-store",
      credentials: "include",
      signal,
    });
    // Headers in hand is the round trip; the body is not part of it.
    samples.push(performance.now() - started);
    // oxlint-disable-next-line no-await-in-loop -- drain before the next sample
    await response.arrayBuffer();
  }
  // The first request may pay for a connection; drop it when there are enough.
  const steady = samples.length > 2 ? samples.slice(1) : samples;
  let jitter = 0;
  for (let index = 1; index < steady.length; index += 1) {
    jitter += Math.abs((steady[index] ?? 0) - (steady[index - 1] ?? 0));
  }
  return {
    jitterMs: steady.length > 1 ? jitter / (steady.length - 1) : 0,
    latencyMs: median(steady),
  };
}

/**
 * Downloads the instance maximum and times it from the first byte, so the
 * round trip before it is not counted as slow transfer.
 */
export async function measureDownload(
  signal: AbortSignal,
  onProgress: (progress: TransferProgress) => void
): Promise<TransferResult> {
  const response = await fetch(DOWNLOAD_URL, {
    cache: "no-store",
    credentials: "include",
    signal,
  });
  if (!response.ok || !response.body) {
    throw new SpeedTestError(await errorBody(response));
  }

  const reader = response.body.getReader();
  const started = performance.now();
  let bytes = 0;
  for (;;) {
    // oxlint-disable-next-line no-await-in-loop -- a stream is read one chunk at a time
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    bytes += value.byteLength;
    onProgress({
      bytes,
      mbps: toMbps(bytes, (performance.now() - started) / 1000),
    });
  }
  const seconds = (performance.now() - started) / 1000;
  return { bytes, mbps: toMbps(bytes, seconds), seconds };
}

/** Incompressible bytes, so a compressing proxy cannot flatter the result. */
export function randomBytes(size: number): Uint8Array<ArrayBuffer> {
  const bytes = new Uint8Array(size);
  for (let offset = 0; offset < size; offset += RANDOM_CHUNK) {
    crypto.getRandomValues(bytes.subarray(offset, offset + RANDOM_CHUNK));
  }
  return bytes;
}

/**
 * Uploads `size` bytes. XHR, not fetch: only XHR reports upload progress, and
 * its first progress event marks when bytes actually started leaving.
 */
export function measureUpload(
  size: number,
  signal: AbortSignal,
  onProgress: (progress: TransferProgress) => void
): Promise<TransferResult> {
  const body = randomBytes(size);

  // oxlint-disable-next-line promise/avoid-new -- XHR speaks in events
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    let started = 0;
    let sent = 0;

    const abort = () => xhr.abort();
    signal.addEventListener("abort", abort, { once: true });
    const finish = () => signal.removeEventListener("abort", abort);

    xhr.upload.addEventListener("loadstart", () => {
      started = performance.now();
    });
    xhr.upload.addEventListener("progress", (event) => {
      sent = event.loaded;
      onProgress({
        bytes: sent,
        mbps: toMbps(sent, (performance.now() - started) / 1000),
      });
    });
    xhr.upload.addEventListener("load", () => {
      sent = size;
    });
    xhr.addEventListener("load", () => {
      finish();
      if (xhr.status < 200 || xhr.status >= 300) {
        let parsed: unknown = { code: "internal" };
        try {
          parsed = JSON.parse(xhr.responseText);
        } catch {
          // Keep the generic body.
        }
        reject(new SpeedTestError(parsed));
        return;
      }
      // The server answered after reading everything, so its reply bounds the
      // end; the trip back is small next to megabytes going up.
      const seconds = (performance.now() - started) / 1000;
      resolve({ bytes: sent, mbps: toMbps(sent, seconds), seconds });
    });
    xhr.addEventListener("error", () => {
      finish();
      reject(new TypeError("network error"));
    });
    xhr.addEventListener("abort", () => {
      finish();
      reject(new DOMException("aborted", "AbortError"));
    });

    xhr.open("POST", UPLOAD_URL);
    xhr.withCredentials = true;
    xhr.setRequestHeader("Content-Type", "application/octet-stream");
    xhr.send(body);
  });
}
