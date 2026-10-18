import { expect, test } from "./fixtures";
import type { Extension } from "./fixtures";
import {
  describeShare,
  env,
  registerUser,
  setEncryption,
  tokensOf,
} from "./instance";
import type { Account } from "./instance";

const isMinted = (token: { name: string }) =>
  token.name.startsWith("Browser extension");

/** Signs in through the site's own page, as a person would. */
async function signInThroughSite(extension: Extension, account: Account) {
  const options = await extension.open("options");
  const [tab] = await Promise.all([
    options.context().waitForEvent("page"),
    options.getByRole("button", { name: "Sign in" }).click(),
  ]);
  await tab.waitForURL(/\/login/u);
  await tab.locator("#identifier").fill(account.username);
  await tab.locator("#password").fill(account.password);
  await tab.locator('button[type="submit"]').click();
  // The extension closes the tab once it holds a token.
  await tab.waitForEvent("close", { timeout: 20_000 });
  await expect(
    options.getByText(`Signed in as ${account.username}`)
  ).toBeVisible();
  return options;
}

test.describe("account", () => {
  test.beforeEach(async ({ extension }) => {
    await setEncryption(false);
    await extension.useTestServer();
  });

  test("signing in on the site mints a token, and signing out revokes it", async ({
    extension,
  }) => {
    const account = await registerUser();
    const options = await signInThroughSite(extension, account);
    const tokens = await tokensOf(account);
    expect(tokens.filter(isMinted)).toEqual([
      expect.objectContaining({ revoked_at: null }),
    ]);

    await options.bringToFront();
    await options.getByRole("button", { name: "Sign out" }).click();
    await expect(options.getByText("Signed out").first()).toBeVisible();
    const after = await tokensOf(account);
    const minted = after.find(isMinted);
    expect(minted?.revoked_at).not.toBeNull();
  });

  test("an upload while signed in is the account's, listed under My shares and deletable there", async ({
    extension,
  }) => {
    const account = await registerUser();
    await signInThroughSite(extension, account);

    const popup = await extension.open("popup");
    await expect(
      popup.getByText(account.username, { exact: true })
    ).toBeVisible();
    await expect(
      popup.getByRole("button", { name: "Open the site" })
    ).toBeVisible();
    // An ordinary account has no admin page to open.
    await expect(
      popup.getByRole("button", { name: "Open the admin panel" })
    ).toHaveCount(0);
    const before = await extension.jobs();
    await popup.getByRole("button", { name: "Upload text" }).click();
    await popup.getByLabel("Upload text").fill("owned by the account");
    await popup.getByRole("button", { name: "Upload note" }).click();
    const job = await extension.nextJob(before);
    expect(job.state, job.error).toBe("done");

    await popup.getByRole("tab", { name: "My shares" }).click();
    const row = popup
      .getByRole("tabpanel")
      .getByRole("listitem")
      .filter({ hasText: "owned by the account" });
    await row.getByRole("button", { name: "Delete share" }).click();
    await row.getByRole("button", { name: "Delete?" }).click();
    await expect(popup.getByText("Share deleted").first()).toBeVisible();
    expect(await describeShare(job.link ?? "")).toBeNull();
  });

  test("an account that may use the admin panel gets a button that opens it", async ({
    context,
    extension,
  }) => {
    await signInThroughSite(extension, {
      password: env().adminPassword,
      username: "admin",
    });
    const popup = await extension.open("popup");
    const admin = popup.getByRole("button", { name: "Open the admin panel" });
    await expect(admin).toBeVisible();
    const [tab] = await Promise.all([
      context.waitForEvent("page"),
      admin.click(),
    ]);
    await tab.waitForURL(`${env().server}/admin**`);
  });
});
