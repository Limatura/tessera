import * as m from "@/paraglide/messages";

/** S3 bucket naming: 3-63 of lowercase letters, digits, dots and hyphens, alphanumeric at both ends. */
const S3_BUCKET = /^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$/u;
/** GCS adds underscores; the same 3-63 length. */
const GCS_BUCKET = /^[a-z0-9][a-z0-9._-]{1,61}[a-z0-9]$/u;
/** Azure container naming: 3-63 of lowercase letters, digits and hyphens, no leading or trailing hyphen. */
const AZURE_CONTAINER = /^[a-z0-9][a-z0-9-]{1,61}[a-z0-9]$/u;
const SCHEME = /^(?<scheme>[a-z][a-z0-9+.-]*):\/\//iu;
const WHITESPACE = /\s/u;

/**
 * The format of a storage backend URL, before the server is asked to open it.
 *
 * Mirrors the schemes `storage::build` accepts (`object_store`'s own list, within the
 * features this binary is built with): a plain path, `file://`, `memory://`, `s3://`,
 * `gs://`, `az://` and friends, `http(s)://`. Whether the credentials work or the
 * bucket exists only the server can tell, when it opens the store before saving.
 * Returns the message for the first problem, or `null`.
 */
export function backendUrlProblem(value: string): string | null {
  const url = value.trim();
  if (url === "") {
    return null;
  }
  if (WHITESPACE.test(url)) {
    return m.settings_storage_backends_url_whitespace();
  }

  const scheme = SCHEME.exec(url)?.groups?.scheme?.toLowerCase();
  // No scheme at all is a local directory, which the server creates if missing.
  if (scheme === undefined) {
    return null;
  }

  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return m.settings_storage_backends_url_malformed();
  }
  return schemeProblem(scheme, parsed);
}

/** The rules that differ per scheme. */
function schemeProblem(scheme: string, parsed: URL): string | null {
  switch (scheme) {
    case "s3":
    case "s3a": {
      return s3Problem(parsed);
    }
    case "gs": {
      return gcsProblem(parsed);
    }
    case "az":
    case "adl":
    case "azure": {
      return azureProblem(parsed.hostname) ?? noCredentialsProblem(parsed);
    }
    case "abfs":
    case "abfss": {
      return abfsProblem(parsed);
    }
    case "http":
    case "https": {
      return parsed.hostname === ""
        ? m.settings_storage_backends_url_host()
        : null;
    }
    case "memory": {
      return null;
    }
    case "file": {
      return fileProblem(parsed);
    }
    default: {
      return m.settings_storage_backends_url_scheme();
    }
  }
}

function s3Problem(parsed: URL): string | null {
  return S3_BUCKET.test(parsed.hostname)
    ? noCredentialsProblem(parsed)
    : m.settings_storage_backends_url_s3_bucket();
}

function gcsProblem(parsed: URL): string | null {
  return GCS_BUCKET.test(parsed.hostname)
    ? noCredentialsProblem(parsed)
    : m.settings_storage_backends_url_gcs_bucket();
}

/** `abfs://container@account.dfs.core.windows.net/path` */
function abfsProblem(parsed: URL): string | null {
  return parsed.username === "" || parsed.hostname === ""
    ? m.settings_storage_backends_url_abfs()
    : azureProblem(parsed.username);
}

/**
 * `file:///data` is a path; `file://data` has `data` as the host, which the server
 * refuses rather than guess.
 */
function fileProblem(parsed: URL): string | null {
  return parsed.hostname === "" && parsed.pathname.length > 1
    ? null
    : m.settings_storage_backends_url_file();
}

function azureProblem(container: string): string | null {
  return AZURE_CONTAINER.test(container)
    ? null
    : m.settings_storage_backends_url_azure_container();
}

/**
 * These carry their credentials in options or the environment, not in the URL:
 * `object_store` takes only the bucket from `s3://key:secret@bucket` and drops
 * the rest without a word.
 */
function noCredentialsProblem(parsed: URL): string | null {
  return parsed.username === "" && parsed.password === ""
    ? null
    : m.settings_storage_backends_url_no_credentials();
}
