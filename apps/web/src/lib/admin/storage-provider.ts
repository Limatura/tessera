import {
  IconBrandAws,
  IconBrandAzure,
  IconBrandCloudflare,
  IconBrandGoogle,
  IconCloud,
  IconCpu,
  IconFolder,
  IconWorld,
} from "@tabler/icons-react";
import type { Icon } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";

type Backend = components["schemas"]["BackendResponse"];

export interface StorageProvider {
  icon: Icon;
  /** Tailwind text color: the brand's own, or a neutral one for generic stores. */
  color: string;
}

const AWS: StorageProvider = { color: "text-[#FF9900]", icon: IconBrandAws };
const AZURE: StorageProvider = {
  color: "text-[#0078D4]",
  icon: IconBrandAzure,
};
const CLOUDFLARE: StorageProvider = {
  color: "text-[#F38020]",
  icon: IconBrandCloudflare,
};
const GOOGLE: StorageProvider = {
  color: "text-[#4285F4]",
  icon: IconBrandGoogle,
};
/** Garage, SeaweedFS, Backblaze and the rest behind `s3://` with an endpoint. */
const S3_COMPATIBLE: StorageProvider = {
  color: "text-sky-500",
  icon: IconCloud,
};
const LOCAL: StorageProvider = { color: "text-amber-500", icon: IconFolder };
const MEMORY: StorageProvider = { color: "text-violet-500", icon: IconCpu };
const HTTP: StorageProvider = { color: "text-emerald-500", icon: IconWorld };

const SCHEME = /^(?<scheme>[a-z][a-z0-9+.-]*):\/\//iu;

/**
 * Which provider a backend is on, for the logo on its card.
 *
 * `s3://` alone does not say AWS: Garage and R2 use it too, with the server in
 * the endpoint option. So the endpoint decides first, and a bucket scheme with
 * no endpoint is the provider's own cloud. A host named in an `https://` URL
 * is matched the same way.
 */
export function storageProviderOf(backend: Backend): StorageProvider {
  const scheme = SCHEME.exec(backend.url)?.groups?.scheme?.toLowerCase();
  if (scheme === undefined || scheme === "file") {
    return LOCAL;
  }
  if (scheme === "memory") {
    return MEMORY;
  }

  const endpoint = backend.config.endpoint ?? null;
  const host = (endpoint && hostOf(endpoint)) || hostOf(backend.url);
  const byHost = host ? providerOfHost(host) : undefined;
  if (byHost) {
    return byHost;
  }

  return providerOfScheme(scheme, endpoint !== null);
}

/** The provider a scheme implies, once the host has said nothing more specific. */
function providerOfScheme(
  scheme: string,
  hasEndpoint: boolean
): StorageProvider {
  switch (scheme) {
    case "s3":
    case "s3a": {
      return hasEndpoint ? S3_COMPATIBLE : AWS;
    }
    case "gs": {
      return GOOGLE;
    }
    case "az":
    case "adl":
    case "azure":
    case "abfs":
    case "abfss": {
      return AZURE;
    }
    default: {
      return HTTP;
    }
  }
}

function providerOfHost(host: string): StorageProvider | undefined {
  if (host.endsWith(".r2.cloudflarestorage.com")) {
    return CLOUDFLARE;
  }
  if (host.endsWith(".amazonaws.com")) {
    return AWS;
  }
  if (host === "storage.googleapis.com") {
    return GOOGLE;
  }
  if (host.endsWith(".core.windows.net")) {
    return AZURE;
  }
  return undefined;
}

function hostOf(url: string): string | undefined {
  try {
    return new URL(url).hostname || undefined;
  } catch {
    return undefined;
  }
}
