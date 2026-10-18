import type { components } from "@tessera/api/schema";

export type ErrorBody = components["schemas"]["ErrorBody"];

/** Which instance to talk to, and as whom. */
export interface UploaderConfig {
  /**
   * The instance's origin, such as `https://files.example`. The web app passes
   * its own `window.location.origin`; anything else passes what its user typed.
   */
  baseUrl: string;
  /**
   * `include` for a cookie session (the web app, which is served by the
   * instance). A client holding an API token sends it through `headers` and
   * leaves this at the default `omit`, so a session in the same browser is
   * never mixed in.
   */
  credentials?: RequestCredentials;
  /**
   * Headers every request to the instance carries, such as `Authorization`.
   * Read per request, so a token swapped mid-way applies to the next one.
   * Never sent to a store's direct-part URL.
   */
  headers?: () => Record<string, string>;
}

/** A configured instance, as every request in this package reads it. */
export interface Context {
  baseUrl: string;
  credentials: RequestCredentials;
  headers: () => Record<string, string>;
}

export function toContext(config: UploaderConfig): Context {
  return {
    // `new URL(path, base)` drops whatever path the base has, so a trailing
    // slash or `/s/ABC` pasted along with the host changes nothing.
    baseUrl: new URL(config.baseUrl).origin,
    credentials: config.credentials ?? "omit",
    headers: config.headers ?? (() => ({})),
  };
}

export function apiUrl(context: Context, path: string): string {
  return new URL(path, context.baseUrl).toString();
}

/** `fetch` against the instance, with its credentials and headers. */
export function request(
  context: Context,
  path: string,
  init: Omit<RequestInit, "credentials" | "headers"> & {
    headers?: Record<string, string>;
  } = {}
): Promise<Response> {
  return fetch(apiUrl(context, path), {
    ...init,
    credentials: context.credentials,
    headers: { ...context.headers(), ...init.headers },
  });
}

/**
 * The `Error` thrown on failure. `message` is the server's English (or the
 * status code when there is no body); `code` and `details` are attached so the
 * screen can translate it.
 */
export function apiError(body: ErrorBody | null, fallback: string): Error {
  const error = new Error(body?.message ?? fallback);
  return body
    ? Object.assign(error, { code: body.code, details: body.details })
    : error;
}

/** Throws the refusal a response carries, or returns it when it succeeded. */
export async function accepted(
  response: Response,
  fallback: string
): Promise<Response> {
  if (response.ok) {
    return response;
  }
  const body = (await response.json().catch(() => null)) as ErrorBody | null;
  throw apiError(body, `${fallback}: ${response.status}`);
}

/** One request whose upload progress matters. */
export interface SendRequest {
  method: "POST" | "PATCH";
  path: string;
  headers: Record<string, string>;
  body: Blob | string;
  signal?: AbortSignal;
  onProgress?: (percent: number) => void;
}

export interface SendResponse {
  status: number;
  /** The JSON body, or `null` when there was none. */
  body: unknown;
}

/**
 * Sends a body with upload progress where the platform has a way to report it.
 *
 * `fetch` has no upload progress events, so this uses `XMLHttpRequest` where
 * it exists (a page, Firefox's extension background). A Chrome extension's
 * service worker has no `XMLHttpRequest`, so there it is `fetch`, and progress
 * jumps to 100 when the request finishes; a resumable upload still moves
 * part by part.
 */
export function send(
  context: Context,
  sending: SendRequest
): Promise<SendResponse> {
  if (typeof XMLHttpRequest === "undefined") {
    return sendWithFetch(context, sending);
  }
  return sendWithXhr(context, sending);
}

async function sendWithFetch(
  context: Context,
  sending: SendRequest
): Promise<SendResponse> {
  const response = await request(context, sending.path, {
    body: sending.body,
    headers: sending.headers,
    method: sending.method,
    signal: sending.signal,
  });
  sending.onProgress?.(100);
  const body: unknown = await response.json().catch(() => null);
  return { body, status: response.status };
}

function sendWithXhr(
  context: Context,
  sending: SendRequest
): Promise<SendResponse> {
  const { signal, onProgress } = sending;
  // oxlint-disable-next-line promise/avoid-new -- XHR progress is only available through callbacks
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(signal.reason ?? new DOMException("Aborted", "AbortError"));
      return;
    }

    const xhr = new XMLHttpRequest();
    xhr.open(sending.method, apiUrl(context, sending.path));
    xhr.responseType = "json";
    xhr.withCredentials = context.credentials === "include";
    for (const [name, value] of Object.entries({
      ...context.headers(),
      ...sending.headers,
    })) {
      xhr.setRequestHeader(name, value);
    }

    const handleAbort = () => xhr.abort();
    signal?.addEventListener("abort", handleAbort);
    const cleanup = () => signal?.removeEventListener("abort", handleAbort);

    xhr.upload.addEventListener("progress", (event) => {
      if (event.lengthComputable) {
        onProgress?.((event.loaded / event.total) * 100);
      }
    });

    xhr.addEventListener("abort", () => {
      cleanup();
      reject(new DOMException("Upload cancelled", "AbortError"));
    });
    xhr.addEventListener("error", () => {
      cleanup();
      reject(new Error("Network error during upload"));
    });
    xhr.addEventListener("load", () => {
      cleanup();
      resolve({ body: xhr.response as unknown, status: xhr.status });
    });

    xhr.send(sending.body);
  });
}
