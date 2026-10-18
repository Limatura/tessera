/**
 * The throwaway instance, driven from Node: what a test needs done to the
 * server that is not the extension's business (instance settings, accounts,
 * reading back what was uploaded).
 */
import { randomUUID } from "node:crypto";

export interface Env {
  server: string;
  site: string;
  otherSite: string;
  extension: string;
  adminPassword: string;
}

/** What `global-setup.ts` started, read from the environment it set. */
export function env(): Env {
  return {
    adminPassword: read("TESSERA_E2E_ADMIN_PASSWORD"),
    extension: read("TESSERA_E2E_EXTENSION"),
    otherSite: read("TESSERA_E2E_OTHER_SITE"),
    server: read("TESSERA_E2E_SERVER"),
    site: read("TESSERA_E2E_SITE"),
  };
}

function read(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is not set; is global-setup.ts configured?`);
  }
  return value;
}

export interface Account {
  username: string;
  password: string;
}

/** A session cookie for `account`, from `POST /auth/login`. */
export async function signIn(account: Account): Promise<string> {
  const response = await fetch(`${env().server}/api/v1/auth/login`, {
    body: JSON.stringify({
      identifier: account.username,
      password: account.password,
    }),
    headers: { "content-type": "application/json" },
    method: "POST",
  });
  if (!response.ok) {
    throw new Error(`sign-in as ${account.username}: ${response.status}`);
  }
  return response.headers.get("set-cookie")?.split(";")[0] ?? "";
}

/** Turns the instance's end-to-end encryption on or off, as its administrator. */
export async function setEncryption(enabled: boolean): Promise<void> {
  const cookie = await signIn({
    password: env().adminPassword,
    username: "admin",
  });
  const response = await fetch(`${env().server}/api/v1/admin/settings`, {
    body: JSON.stringify({ changes: { "features.encryption": enabled } }),
    headers: { "content-type": "application/json", cookie },
    method: "PUT",
  });
  if (!response.ok) {
    throw new Error(
      `features.encryption: ${response.status} ${await response.text()}`
    );
  }
}

/** A new ordinary account, with the role registration gives everyone. */
export async function registerUser(): Promise<Account> {
  const username = `user${randomUUID().slice(0, 8)}`;
  const account = { password: "an ordinary passphrase", username };
  const response = await fetch(`${env().server}/api/v1/auth/register`, {
    body: JSON.stringify({ ...account, email: `${username}@example.com` }),
    headers: { "content-type": "application/json" },
    method: "POST",
  });
  if (response.status !== 201) {
    throw new Error(`register: ${response.status} ${await response.text()}`);
  }
  return account;
}

export interface TokenSummary {
  name: string;
  revoked_at: string | null;
}

export async function tokensOf(account: Account): Promise<TokenSummary[]> {
  const cookie = await signIn(account);
  const response = await fetch(`${env().server}/api/v1/tokens`, {
    headers: { cookie },
  });
  return (await response.json()) as TokenSummary[];
}

export interface DescribedShare {
  kind: string;
  encryption: string | null;
  has_password: boolean;
  title: string | null;
  language: string | null;
  files: { filename: string | null; size_bytes: number }[];
}

/** `GET /shares/{code}` for the share a link points at; `null` once it is gone. */
export async function describeShare(
  link: string
): Promise<DescribedShare | null> {
  const code = new URL(link).pathname.split("/").at(-1) ?? "";
  const response = await fetch(`${env().server}/api/v1/shares/${code}`);
  if (response.status === 404) {
    return null;
  }
  return (await response.json()) as DescribedShare;
}

/** The text of an unsealed note, fetched the way the CLI does: unlock, then `/raw`. */
export async function readNote(link: string): Promise<string> {
  const code = new URL(link).pathname.split("/").at(-1) ?? "";
  const unlocked = await fetch(`${env().server}/api/v1/shares/${code}/unlock`, {
    method: "POST",
  });
  const { ticket } = (await unlocked.json()) as { ticket: string };
  const raw = await fetch(
    `${env().server}/s/${code}/raw?t=${encodeURIComponent(ticket)}`
  );
  return raw.text();
}
