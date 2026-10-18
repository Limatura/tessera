/**
 * Talking to the configured instance: its address, the permission to reach
 * it, and clients for the API that carry the user's token.
 */

import type { components, paths } from "@tessera/api/schema";
import { createUploader } from "@tessera/upload";
import type { Uploader } from "@tessera/upload";
import createClient from "openapi-fetch";

import { browser } from "#imports";

import type { Credential } from "./storage";
import { currentCredential, serverItem } from "./storage";

export type Capabilities = components["schemas"]["Capabilities"];
export type Account = components["schemas"]["AccountResponse"];

/**
 * What the user typed, as an origin. A bare host gets `https://`; a pasted
 * share link or a trailing path is cut back to the origin, which is all the
 * API needs. `null` when it is not an address at all.
 */
export function normaliseServer(input: string): string | null {
  const trimmed = input.trim();
  if (!trimmed) {
    return null;
  }
  const withScheme = /^[a-z][a-z\d+.-]*:\/\//iu.test(trimmed)
    ? trimmed
    : `https://${trimmed}`;
  try {
    const url = new URL(withScheme);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      return null;
    }
    return url.origin;
  } catch {
    return null;
  }
}

/** The match pattern that grants the extension requests to `server`. */
export function originPattern(server: string): string {
  return `${server}/*`;
}

export function hasServerPermission(server: string): Promise<boolean> {
  return browser.permissions.contains({ origins: [originPattern(server)] });
}

/**
 * Asks for the right to reach `server`. Must run during a click: browsers
 * refuse a permission prompt that no gesture asked for.
 */
export function requestServerPermission(server: string): Promise<boolean> {
  return browser.permissions.request({ origins: [originPattern(server)] });
}

/** A typed client for `server`, signed in when there is a token for it. */
export function apiClient(server: string, credential: Credential | null) {
  return createClient<paths>({
    baseUrl: server,
    // A token, never the browser's cookies: a session the user has in a tab
    // must not decide who an upload from the extension belongs to.
    credentials: "omit",
    headers: credential ? { Authorization: `Bearer ${credential.token}` } : {},
  });
}

export function uploaderFor(
  server: string,
  credential: Credential | null
): Uploader {
  return createUploader({
    baseUrl: server,
    headers: (): Record<string, string> =>
      credential ? { Authorization: `Bearer ${credential.token}` } : {},
  });
}

/** The configured instance and the token for it, if any. */
export async function connection(): Promise<{
  server: string;
  credential: Credential | null;
} | null> {
  const server = await serverItem.getValue();
  if (!server) {
    return null;
  }
  return { credential: await currentCredential(), server };
}

/**
 * What the instance allows this caller. Per caller, not per instance: the
 * largest file and the room left depend on who asks.
 */
export async function fetchCapabilities(
  server: string,
  credential: Credential | null
): Promise<Capabilities> {
  const { data, error, response } = await apiClient(server, credential).GET(
    "/api/v1/capabilities"
  );
  if (!data) {
    throw new Error(errorMessage(error) ?? `capabilities: ${response.status}`);
  }
  return data;
}

/** The server's own words from an error body, when it sent one. */
export function errorMessage(error: unknown): string | undefined {
  if (typeof error === "object" && error !== null && "message" in error) {
    const { message } = error as { message: unknown };
    return typeof message === "string" ? message : undefined;
  }
  return undefined;
}
