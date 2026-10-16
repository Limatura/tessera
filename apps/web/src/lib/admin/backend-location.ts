import type { components } from "@tessera/api/schema";

type Backend = components["schemas"]["BackendResponse"];

/**
 * What kind of store a backend is, as the editor asks it. `other` takes a raw
 * URL and leaves the stored options alone, for anything the others cannot say
 * (an `https://` AWS URL, `abfs://`, `memory://`).
 */
export type LocationKind =
  | "local"
  | "s3"
  | "gcs"
  | "azure"
  | "webdav"
  | "other";

export const LOCATION_KINDS: LocationKind[] = [
  "s3",
  "local",
  "gcs",
  "azure",
  "webdav",
  "other",
];

/**
 * Every field any kind uses, flat, so switching kinds keeps what was typed.
 * Secrets start blank: the server never sends them, and blank means keep the
 * stored one.
 */
export interface LocationFields {
  kind: LocationKind;
  /** local */
  path: string;
  /** s3, gcs; the container for azure */
  bucket: string;
  /** s3, gcs, azure */
  prefix: string;
  /** s3: blank is AWS itself */
  endpoint: string;
  /** s3 */
  region: string;
  /** s3 */
  accessKeyId: string;
  /** s3 */
  secretAccessKey: string;
  /** azure */
  account: string;
  /** azure */
  accountKey: string;
  /** gcs: the service account's JSON key */
  serviceAccountKey: string;
  /** webdav, other */
  url: string;
  /** webdav */
  username: string;
  /** webdav */
  password: string;
}

/** What `POST`/`PUT` take. `options` absent keeps the stored ones. */
export interface LocationRequest {
  url: string;
  options?: Record<string, string>;
}

const SCHEME = /^(?<scheme>[a-z][a-z0-9+.-]*):\/\//iu;
const LEADING_SLASHES = /^\/+/u;
const TRAILING_SLASHES = /\/+$/u;

const EMPTY: LocationFields = {
  accessKeyId: "",
  account: "",
  accountKey: "",
  bucket: "",
  endpoint: "",
  kind: "s3",
  password: "",
  path: "",
  prefix: "",
  region: "",
  secretAccessKey: "",
  serviceAccountKey: "",
  url: "",
  username: "",
};

/** The editor's starting fields: blank for a new backend, read back for an existing one. */
export function locationFieldsOf(backend: Backend | null): LocationFields {
  if (backend === null) {
    return EMPTY;
  }
  const scheme = SCHEME.exec(backend.url)?.groups?.scheme?.toLowerCase();
  if (scheme === undefined || scheme === "file") {
    return { ...EMPTY, kind: "local", path: backend.url };
  }

  return remoteFieldsOf(scheme, backend);
}

/** The fields for anything with a scheme other than `file://`. */
function remoteFieldsOf(scheme: string, backend: Backend): LocationFields {
  const parsed = safeUrl(backend.url);
  const bucket = parsed?.hostname ?? "";
  const prefix = parsed ? trimSlashes(decodeURIComponent(parsed.pathname)) : "";
  const { config } = backend;

  switch (scheme) {
    case "s3":
    case "s3a": {
      return {
        ...EMPTY,
        bucket,
        endpoint: config.endpoint ?? "",
        kind: "s3",
        prefix,
        region: config.region ?? "",
      };
    }
    case "gs": {
      return { ...EMPTY, bucket, kind: "gcs", prefix };
    }
    case "az":
    case "azure":
    case "adl": {
      return {
        ...EMPTY,
        ...azureFields(parsed, config.account),
        kind: "azure",
        prefix,
      };
    }
    case "http":
    case "https": {
      return { ...EMPTY, kind: "webdav", url: backend.url };
    }
    default: {
      return { ...EMPTY, kind: "other", url: backend.url };
    }
  }
}

/**
 * `az://container@account.blob.core.windows.net/path` (what the CLI writes)
 * puts the container before the `@` and the account in the host; the form
 * `az://container/path` has the container as the host and the account in the
 * stored options.
 */
function azureFields(
  parsed: URL | null,
  storedAccount: string | null | undefined
): Pick<LocationFields, "account" | "bucket"> {
  if (parsed !== null && parsed.username !== "") {
    return {
      account: parsed.hostname.split(".")[0],
      bucket: decodeURIComponent(parsed.username),
    };
  }
  return { account: storedAccount ?? "", bucket: parsed?.hostname ?? "" };
}

/**
 * The URL and options to send.
 *
 * Credentials left blank are left out, and the server keeps the stored ones as
 * long as nothing else that places the store changed. `other` sends no options
 * at all, which keeps whatever is stored.
 */
export function locationRequestOf(fields: LocationFields): LocationRequest {
  switch (fields.kind) {
    case "local": {
      return { options: {}, url: fields.path.trim() };
    }
    case "s3": {
      return { options: s3Options(fields), url: bucketUrl("s3", fields) };
    }
    case "gcs": {
      return {
        options: pick({ google_service_account_key: fields.serviceAccountKey }),
        url: bucketUrl("gs", fields),
      };
    }
    case "azure": {
      return {
        options: pick({
          azure_storage_account_key: fields.accountKey,
          azure_storage_account_name: fields.account,
        }),
        url: bucketUrl("az", fields),
      };
    }
    case "webdav": {
      return { options: {}, url: webdavUrl(fields) };
    }
    default: {
      return { url: fields.url.trim() };
    }
  }
}

/** Which required fields are missing, as field names; empty when it can be sent. */
export function missingLocationFields(
  fields: LocationFields
): (keyof LocationFields)[] {
  const required: Record<LocationKind, (keyof LocationFields)[]> = {
    azure: ["bucket", "account"],
    gcs: ["bucket"],
    local: ["path"],
    other: ["url"],
    s3: ["bucket"],
    webdav: ["url"],
  };
  return required[fields.kind].filter(
    (key) => String(fields[key]).trim() === ""
  );
}

/** Fields that only work together, by kind. */
const PAIRS: Partial<
  Record<LocationKind, [keyof LocationFields, keyof LocationFields][]>
> = {
  s3: [["accessKeyId", "secretAccessKey"]],
  webdav: [["username", "password"]],
};

/**
 * Pairs that only work together: an access key with no secret, or a user with
 * no password, can never sign in. Both blank is fine: it keeps the stored pair.
 */
export function halfPairedField(
  fields: LocationFields
): keyof LocationFields | null {
  const pairs = PAIRS[fields.kind] ?? [];
  for (const [first, second] of pairs) {
    const hasFirst = String(fields[first]).trim() !== "";
    const hasSecond = String(fields[second]).trim() !== "";
    if (hasFirst !== hasSecond) {
      return hasFirst ? second : first;
    }
  }
  return null;
}

/** Whether a custom endpoint is a usable `http(s)://` URL; blank is AWS and fine. */
export function endpointIsValid(endpoint: string): boolean {
  const value = endpoint.trim();
  if (value === "") {
    return true;
  }
  const parsed = safeUrl(value);
  return (
    parsed !== null &&
    (parsed.protocol === "http:" || parsed.protocol === "https:") &&
    parsed.hostname !== ""
  );
}

/**
 * Whether saving points the backend somewhere its files are not: a new URL,
 * or for S3 a new endpoint behind the same `s3://` URL.
 */
export function movesLocation(
  backend: Backend | null,
  fields: LocationFields
): boolean {
  if (backend === null) {
    return false;
  }
  const { url } = locationRequestOf(fields);
  if (url === "") {
    return false;
  }
  if (withoutCredentials(url) !== backend.url) {
    return true;
  }
  return (
    fields.kind === "s3" &&
    trimTrailing(fields.endpoint.trim()) !==
      trimTrailing(backend.config.endpoint ?? "")
  );
}

function s3Options(fields: LocationFields): Record<string, string> {
  const endpoint = fields.endpoint.trim();
  return pick({
    aws_access_key_id: fields.accessKeyId,
    aws_allow_http: endpoint.startsWith("http://") ? "true" : "",
    aws_endpoint: endpoint,
    aws_region: fields.region,
    aws_secret_access_key: fields.secretAccessKey,
  });
}

function bucketUrl(scheme: string, fields: LocationFields): string {
  const bucket = fields.bucket.trim();
  const prefix = trimSlashes(fields.prefix.trim());
  return prefix === ""
    ? `${scheme}://${bucket}`
    : `${scheme}://${bucket}/${prefix}`;
}

function webdavUrl(fields: LocationFields): string {
  const url = fields.url.trim();
  const username = fields.username.trim();
  if (username === "" || fields.password === "") {
    return url;
  }
  const parsed = safeUrl(url);
  if (parsed === null) {
    return url;
  }
  parsed.username = username;
  parsed.password = fields.password;
  return parsed.toString();
}

/** The non-blank entries, trimmed. */
function pick(entries: Record<string, string>): Record<string, string> {
  return Object.fromEntries(
    Object.entries(entries)
      .map(([key, value]) => [key, value.trim()] as const)
      .filter(([, value]) => value !== "")
  );
}

function withoutCredentials(value: string): string {
  const parsed = safeUrl(value);
  if (parsed === null || (parsed.username === "" && parsed.password === "")) {
    return value;
  }
  parsed.username = "";
  parsed.password = "";
  return parsed.toString();
}

function safeUrl(value: string): URL | null {
  try {
    return new URL(value);
  } catch {
    return null;
  }
}

function trimSlashes(value: string): string {
  return value.replace(LEADING_SLASHES, "").replace(TRAILING_SLASHES, "");
}

function trimTrailing(value: string): string {
  return value.replace(TRAILING_SLASHES, "");
}
