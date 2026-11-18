/**
 * Measuring the line to this instance from the browser.
 *
 * The server only moves bytes (`/api/v1/speed/*`); every clock reading here is
 * the browser's. Each direction is one test against `auth.throttle.speed`
 * (5 an hour per source by default), so a run spends two (the upload's parts
 * count as one) and latency is taken from `/capabilities`, which is not counted.
 */

const DOWNLOAD_URL = "/api/v1/speed/download";
const UPLOAD_URL = "/api/v1/speed/upload";
const PING_URL = "/api/v1/capabilities";

/**
 * The start of a transfer left out of its result, and of the live reading: the
 * connection is still finding its speed, which every speed test trims.
 */
export const RAMP_UP_MS = 600;

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

/**
 * Progress of one direction: bytes so far, and for an upload the rate of the
 * last part (0 until there is a steady one). A download's `mbps` is the average
 * so far; read its live rate from the bytes instead.
 */
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
  // Where the steady part begins: the first chunk after the ramp-up.
  let steadyAt = 0;
  let steadyBytes = 0;
  for (;;) {
    // oxlint-disable-next-line no-await-in-loop -- a stream is read one chunk at a time
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    bytes += value.byteLength;
    const now = performance.now();
    if (steadyAt === 0 && now - started >= RAMP_UP_MS) {
      steadyAt = now;
      steadyBytes = bytes;
    }
    onProgress({ bytes, mbps: toMbps(bytes, (now - started) / 1000) });
  }
  const ended = performance.now();
  // A transfer over before the ramp-up ended has nothing steady to trim to.
  const from = steadyAt > 0 && steadyAt < ended ? steadyAt : started;
  const counted = from === started ? bytes : bytes - steadyBytes;
  const seconds = (ended - from) / 1000;
  return { bytes, mbps: toMbps(counted, seconds), seconds };
}

/** Incompressible bytes, so a compressing proxy cannot flatter the result. */
export function randomBytes(size: number): Uint8Array<ArrayBuffer> {
  const bytes = new Uint8Array(size);
  for (let offset = 0; offset < size; offset += RANDOM_CHUNK) {
    crypto.getRandomValues(bytes.subarray(offset, offset + RANDOM_CHUNK));
  }
  return bytes;
}

/** How many requests one upload test is split into. */
const UPLOAD_PARTS = 10;
/** Parts smaller than this are mostly request overhead. */
const MIN_PART_BYTES = 256 * 1024;

interface PartReply {
  received_bytes: number;
  run: string;
  remaining_bytes: number;
}

/**
 * Uploads `size` bytes in parts and times each by its reply.
 *
 * Not one request: upload progress events count bytes handed to the socket,
 * and a buffering proxy takes megabytes at once, so they say little about the
 * line. A reply only comes once the server has read the part, so it is the
 * honest clock; the server counts the parts as one test (`run`).
 *
 * Each part's time loses one round trip (`rttMs`), the wait for the reply that
 * is not transfer. The first part is left out of the result: the connection is
 * still ramping up, which every speed test trims.
 *
 * Between replies, progress is estimated from the last part's rate, so a dial
 * can move smoothly without trusting the socket's account.
 */
export async function measureUpload(
  size: number,
  rttMs: number,
  signal: AbortSignal,
  onProgress: (progress: TransferProgress) => void
): Promise<TransferResult> {
  const partSize = Math.max(MIN_PART_BYTES, Math.ceil(size / UPLOAD_PARTS));
  const body = randomBytes(Math.min(partSize, size));
  const started = performance.now();
  let run: string | undefined;
  let sent = 0;
  let lastRate = 0;
  let measuredBytes = 0;
  let measuredSeconds = 0;
  let part = 0;

  while (sent < size) {
    const length = Math.min(partSize, size - sent);
    const partStarted = performance.now();
    const before = sent;
    const rate = lastRate;
    // The rate to estimate from is the previous part's, and the first part's
    // is still the ramp-up.
    const steady = part >= 2;
    // oxlint-disable-next-line no-await-in-loop -- parts share one allowance, so they go one at a time
    const reply = await sendPart(body.subarray(0, length), run, signal, () => {
      if (!steady || rate <= 0) {
        return;
      }
      const elapsed = (performance.now() - partStarted) / 1000;
      const estimate = Math.min(length, (rate * elapsed * 1_000_000) / 8);
      onProgress({ bytes: before + estimate, mbps: rate });
    });
    const seconds = (performance.now() - partStarted) / 1000;
    // The reply's trip back is not transfer; never let it take more than half.
    const transfer = Math.max(seconds - rttMs / 1000, seconds / 2);
    ({ run } = reply);
    sent += reply.received_bytes;
    lastRate = toMbps(reply.received_bytes, transfer);
    if (part > 0) {
      measuredBytes += reply.received_bytes;
      measuredSeconds += transfer;
    }
    part += 1;
    // The first part's rate is still the ramp-up; the reading starts after it.
    if (part > 1) {
      onProgress({ bytes: sent, mbps: lastRate });
    } else {
      onProgress({ bytes: sent, mbps: 0 });
    }
    if (reply.remaining_bytes === 0 || reply.received_bytes === 0) {
      break;
    }
  }

  // A test of a single part has no steady part to report.
  if (measuredSeconds === 0) {
    measuredBytes = sent;
    measuredSeconds = (performance.now() - started) / 1000;
  }
  return {
    bytes: sent,
    mbps: toMbps(measuredBytes, measuredSeconds),
    seconds: measuredSeconds,
  };
}

/**
 * One part. XHR, not fetch, for `upload.progress`: not trusted for bytes, only
 * as a tick while the part is out.
 */
function sendPart(
  body: Uint8Array<ArrayBuffer>,
  run: string | undefined,
  signal: AbortSignal,
  onTick: () => void
): Promise<PartReply> {
  // oxlint-disable-next-line promise/avoid-new -- XHR speaks in events
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    const abort = () => xhr.abort();
    signal.addEventListener("abort", abort, { once: true });
    const finish = () => signal.removeEventListener("abort", abort);

    xhr.upload.addEventListener("progress", onTick);
    xhr.addEventListener("load", () => {
      finish();
      let parsed: unknown = { code: "internal" };
      try {
        parsed = JSON.parse(xhr.responseText);
      } catch {
        // Keep the generic body.
      }
      if (xhr.status < 200 || xhr.status >= 300) {
        reject(new SpeedTestError(parsed));
        return;
      }
      resolve(parsed as PartReply);
    });
    xhr.addEventListener("error", () => {
      finish();
      reject(new TypeError("network error"));
    });
    xhr.addEventListener("abort", () => {
      finish();
      reject(new DOMException("aborted", "AbortError"));
    });

    const url = run
      ? `${UPLOAD_URL}?run=${encodeURIComponent(run)}`
      : UPLOAD_URL;
    xhr.open("POST", url);
    xhr.withCredentials = true;
    xhr.setRequestHeader("Content-Type", "application/octet-stream");
    xhr.send(body);
  });
}

/** Where the gauge's tick labels sit; the dial is not linear, like the ones lines are sold with. */
export const GAUGE_STOPS = [0, 5, 10, 50, 100, 250, 500, 1000] as const;

/**
 * Position on the dial, 0 to 1. Each gap between stops takes the same angle,
 * so 5 Mbps and 500 Mbps are both readable on one scale.
 */
export function gaugeFraction(mbps: number): number {
  const last = GAUGE_STOPS.length - 1;
  if (!(mbps > 0)) {
    return 0;
  }
  for (let index = 1; index <= last; index += 1) {
    const high = GAUGE_STOPS[index] ?? 0;
    if (mbps <= high) {
      const low = GAUGE_STOPS[index - 1] ?? 0;
      return (index - 1 + (mbps - low) / (high - low)) / last;
    }
  }
  return 1;
}

export interface RateSample {
  at: number;
  bytes: number;
}

/**
 * Rate over the most recent `windowMs` of samples, so the dial follows the
 * line now instead of the slow average since the first byte.
 */
export function windowedMbps(samples: RateSample[], windowMs = 400): number {
  const latest = samples.at(-1);
  if (!latest) {
    return 0;
  }
  const from =
    samples.findLast((sample) => latest.at - sample.at >= windowMs) ??
    samples[0];
  if (!from || from === latest) {
    return 0;
  }
  return toMbps(latest.bytes - from.bytes, (latest.at - from.at) / 1000);
}
