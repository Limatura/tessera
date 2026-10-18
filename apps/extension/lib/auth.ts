/**
 * Signing the extension in: the user signs in on the instance's own site,
 * with whatever it offers (password, authenticator code, passkey, an identity
 * provider), and the extension mints an API token for itself from that tab.
 *
 * Why the token is minted inside the tab rather than by the extension: the
 * session cookie is `SameSite=Lax`, and whether a browser sends it on a
 * request from an extension differs between browsers. A request from the
 * instance's own page is same-origin everywhere.
 */
import { i18n } from "#i18n";
import { browser, storage } from "#imports";

import { apiClient, errorMessage } from "./instance";
import { notify } from "./notify";
import { credentialItem, serverItem } from "./storage";
import type { Credential } from "./storage";

/**
 * A page that needs a sign-in: an account already signed in lands on it, and
 * anyone else is sent through `/login` and back to it.
 */
const SIGN_IN_PATH = "/shares";

/**
 * What the token may do: upload, list the account's shares, delete them.
 * Narrowed to what the account itself holds, since the server refuses a token
 * wider than its owner.
 */
const TOKEN_PERMISSIONS = ["shares:create", "shares:read", "shares:delete"];

interface PendingSignIn {
  tabId: number;
  server: string;
}

/** The tab a sign-in is waiting on. In storage so a restarted worker still knows. */
const pendingItem = storage.defineItem<PendingSignIn | null>(
  "session:pendingSignIn",
  { fallback: null }
);

/** Opens the instance's sign-in in a tab. The background finishes it. */
export async function startSignIn(): Promise<void> {
  const server = await serverItem.getValue();
  if (!server) {
    throw new Error(i18n.t("errors.noServer"));
  }
  const tab = await browser.tabs.create({ url: `${server}${SIGN_IN_PATH}` });
  if (tab.id !== undefined) {
    await pendingItem.setValue({ server, tabId: tab.id });
  }
}

let minting = false;

/**
 * Registered by the background: watches the sign-in tab until it reaches
 * {@link SIGN_IN_PATH} signed in, then mints the token there.
 */
export function watchSignIn(): void {
  browser.tabs.onUpdated.addListener(async (tabId, change, tab) => {
    try {
      await finishSignIn(tabId, change.url ?? tab.url, change.status);
    } catch (error) {
      await pendingItem.setValue(null);
      await notify(
        i18n.t("signIn.failedTitle"),
        error instanceof Error ? error.message : String(error)
      );
    }
  });
  browser.tabs.onRemoved.addListener(async (tabId) => {
    const pending = await pendingItem.getValue();
    if (pending?.tabId === tabId) {
      await pendingItem.setValue(null);
    }
  });
}

async function finishSignIn(
  tabId: number,
  url: string | undefined,
  status: string | undefined
): Promise<void> {
  const pending = await pendingItem.getValue();
  if (!pending || pending.tabId !== tabId || !url || minting) {
    return;
  }
  const { origin, pathname } = new URL(url);
  // The single-page app moves to the page without a load after signing in,
  // so a change of URL counts as much as a finished load.
  if (
    origin !== pending.server ||
    !pathname.startsWith(SIGN_IN_PATH) ||
    (status !== "complete" && status !== undefined)
  ) {
    return;
  }

  minting = true;
  try {
    const [injection] = await browser.scripting.executeScript({
      args: [tokenName(), TOKEN_PERMISSIONS],
      func: mintToken,
      target: { tabId },
      world: "MAIN",
    });
    const result = injection?.result as MintResult | undefined;
    if (!result || result.kind === "signed-out") {
      // Still on its way through the sign-in; the next change tries again.
      return;
    }
    await pendingItem.setValue(null);
    if (result.kind === "error") {
      throw new Error(mintErrorMessage(result));
    }
    await credentialItem.setValue({
      server: pending.server,
      token: result.token,
      tokenId: result.tokenId,
      username: result.username,
    });
    await browser.tabs.remove(tabId).catch(() => {
      // Closed by the user already.
    });
    await notify(
      i18n.t("signIn.doneTitle"),
      i18n.t("signIn.done", [result.username])
    );
  } finally {
    minting = false;
  }
}

type MintResult =
  | { kind: "signed-out" }
  | { kind: "signed-in"; token: string; tokenId: string; username: string }
  | {
      kind: "error";
      reason: "no-upload" | "no-tokens" | "server";
      message?: string;
    };

function mintErrorMessage(result: Extract<MintResult, { kind: "error" }>) {
  switch (result.reason) {
    case "no-upload": {
      return i18n.t("signIn.noUploadPermission");
    }
    case "no-tokens": {
      return i18n.t("signIn.noTokenPermission");
    }
    default: {
      return result.message ?? i18n.t("errors.unknown");
    }
  }
}

/**
 * Runs in the instance's page (the main world, so its requests are the
 * page's own), and is serialised to get there: nothing from this module is in
 * scope.
 */
async function mintToken(name: string, wanted: string[]): Promise<MintResult> {
  const session = await fetch("/api/v1/auth/session", {
    credentials: "include",
  });
  if (session.status === 401) {
    return { kind: "signed-out" };
  }
  if (!session.ok) {
    return {
      kind: "error",
      message: `session: ${session.status}`,
      reason: "server",
    };
  }
  const account = (await session.json()) as {
    username: string;
    permissions: string[];
  };
  if (!account.permissions.includes("shares:create")) {
    return { kind: "error", reason: "no-upload" };
  }
  if (!account.permissions.includes("tokens:manage")) {
    return { kind: "error", reason: "no-tokens" };
  }
  const response = await fetch("/api/v1/tokens", {
    body: JSON.stringify({
      name,
      permissions: wanted.filter((permission) =>
        account.permissions.includes(permission)
      ),
    }),
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    method: "POST",
  });
  const body = (await response.json().catch(() => null)) as {
    id?: string;
    secret?: string;
    message?: string;
  } | null;
  if (!response.ok || !body?.secret || !body.id) {
    return {
      kind: "error",
      message: body?.message ?? `tokens: ${response.status}`,
      reason: "server",
    };
  }
  return {
    kind: "signed-in",
    token: body.secret,
    tokenId: body.id,
    username: account.username,
  };
}

/** What the token is called in the account's list, so its owner knows it. */
function tokenName(): string {
  const browserName =
    import.meta.env.BROWSER === "firefox" ? "Firefox" : "Chrome";
  const date = new Date().toISOString().slice(0, 10);
  return i18n.t("signIn.tokenName", [browserName, date]);
}

/**
 * Uses a token the user created on the site and pasted in. Checked against
 * the instance first, so a typo is caught here and not at the first upload.
 */
export async function signInWithToken(
  server: string,
  token: string
): Promise<Credential> {
  const probe: Credential = { server, token, tokenId: null, username: "" };
  const { data, error } = await apiClient(server, probe).GET(
    "/api/v1/auth/session"
  );
  if (error || !data) {
    throw new Error(errorMessage(error) ?? i18n.t("signIn.badToken"));
  }
  const credential = { ...probe, username: data.username };
  await credentialItem.setValue(credential);
  return credential;
}

/**
 * Forgets the token and, when the extension minted it, tries to revoke it.
 * `true` once nothing the extension held still works.
 *
 * Revoking needs a session, not a token (a leaked token must not be able to
 * revoke its owner's other ones), so it rides on the browser's cookie for the
 * instance. Where that is not sent, the token stays valid until the user
 * revokes it on the site.
 */
export async function signOut(): Promise<boolean> {
  const credential = await credentialItem.getValue();
  await credentialItem.setValue(null);
  if (!credential) {
    return true;
  }
  if (!credential.tokenId) {
    // Pasted in: the extension never knew its id, so it stays valid.
    return false;
  }
  try {
    const response = await fetch(
      `${credential.server}/api/v1/tokens/${encodeURIComponent(credential.tokenId)}`,
      { credentials: "include", method: "DELETE" }
    );
    return response.ok || response.status === 404;
  } catch {
    return false;
  }
}
