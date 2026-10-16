import type { components } from "@tessera/api/schema";

import { readSetting } from "@/hooks/admin/use-settings";

type Settings = components["schemas"]["SettingsResponse"];

/**
 * A `value` from `GET /admin/settings` is opaque JSON whose type depends on the
 * key, so every reader checks it. Anything missing (still loading, a key this
 * server version lacks, or a SECRET, always null) falls back.
 */
export function asString(value: unknown, fallback: string): string {
  return typeof value === "string" ? value : fallback;
}

export function asNumber(value: unknown, fallback: number): number {
  return typeof value === "number" ? value : fallback;
}

export function asBoolean(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

/** A `string[]` such as `scopes`, edited as one space-separated line. */
export function asScopes(value: unknown, fallback: string): string {
  if (!Array.isArray(value)) {
    return fallback;
  }
  return value.filter((item) => typeof item === "string").join(" ");
}

/** Turns the space-separated form line back into the `string[]` the server wants. */
export function toScopes(value: string): string[] {
  return value.split(/\s+/u).filter(Boolean);
}

export function asEnum<const TOptions extends readonly string[]>(
  value: unknown,
  options: TOptions,
  fallback: TOptions[number]
): TOptions[number] {
  return options.find((option) => option === value) ?? fallback;
}

/**
 * A SEALED field reports whether it is set, not its value, so this only decides
 * whether the placeholder says "set" or "not set".
 */
export function isSecretSet(
  settings: Settings | undefined,
  key: string
): boolean {
  return readSetting(settings, key) === true;
}

/** A `string[]` such as `trusted_proxies`, edited one entry per line. */
export function asLines(value: unknown, fallback: string): string {
  if (!Array.isArray(value)) {
    return fallback;
  }
  return value.filter((item) => typeof item === "string").join("\n");
}

/** Turns multi-line text back into a `string[]`, dropping blank lines and outer whitespace. */
export function toLines(value: string): string[] {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}
