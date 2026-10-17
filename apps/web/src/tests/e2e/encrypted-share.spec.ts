import { expect, test } from "@playwright/test";
import type { Page, Request } from "@playwright/test";

// End-to-end encrypted shares, anonymous. Needs a backend started with
// `TESSERA_FEATURES__ENCRYPTION=true`; the switch is off by default.

const SHARE_URL = /\/s\/(?<code>[A-Za-z0-9]+)(?<fragment>#\S+)?/u;
const PASSWORD = "correct horse battery staple";

/** Type a snippet into the code tab and turn encryption on. */
async function prepareSnippet(page: Page, snippet: string) {
  await page.goto("/");
  await page.getByRole("tab", { name: "Code" }).click();
  await page.locator(".cm-content").click();
  await page.keyboard.type(snippet);
  await page.getByRole("switch", { name: "End-to-end encryption" }).click();
}

/**
 * Send it and collect every request body on the way, so the test can check the
 * plaintext never left the browser.
 */
async function send(page: Page) {
  const bodies: string[] = [];
  const collect = (request: Request) => {
    if (request.url().includes("/api/")) {
      bodies.push(request.postDataBuffer()?.toString("latin1") ?? "");
    }
  };
  page.on("request", collect);
  await page.getByRole("button", { name: "Send snippet" }).click();
  await expect(page.getByText("Ready to share")).toBeVisible();
  page.off("request", collect);

  const shown = await page.getByText(SHARE_URL).textContent();
  const match = shown?.match(SHARE_URL);
  expect(match?.groups?.code, `share URL not found in "${shown}"`).toBeTruthy();
  return {
    bodies,
    code: match?.groups?.code as string,
    fragment: match?.groups?.fragment ?? "",
  };
}

async function deleteFromResult(page: Page) {
  await page.getByRole("button", { exact: true, name: "Delete" }).click();
  const dialog = page.getByRole("alertdialog");
  await dialog.getByRole("button", { exact: true, name: "Delete" }).click();
  await expect(page.getByText("Ready to share")).toBeHidden();
}

test("a link-keyed share opens only with the full link", async ({
  context,
  page,
}) => {
  const snippet = `tessera e2e sealed ${Date.now()}`;
  await prepareSnippet(page, snippet);
  const { bodies, code, fragment } = await send(page);

  expect(fragment, "the key travels in the fragment").toMatch(/^#.+/u);
  expect(bodies.length).toBeGreaterThan(0);
  for (const body of bodies) {
    expect(body).not.toContain(snippet);
  }

  const visitor = await context.newPage();
  await visitor.goto(`/s/${code}${fragment}`);
  await expect(visitor.getByText(snippet)).toBeVisible();
  await expect(visitor.getByText("End-to-end encrypted").first()).toBeVisible();

  // Without the fragment the server has nothing to decrypt with.
  await visitor.goto(`/s/${code}`);
  await expect(visitor.getByText("This link is incomplete")).toBeVisible();
  await expect(visitor.getByText(snippet)).toBeHidden();
  await visitor.close();

  await deleteFromResult(page);
});

test("a password-keyed share opens only with its password", async ({
  context,
  page,
}) => {
  const snippet = `tessera e2e password ${Date.now()}`;
  await prepareSnippet(page, snippet);
  await page.getByRole("switch", { name: "Use a password as the key" }).click();
  await page
    .getByLabel(/^Password \(at least \d+ characters\)$/u)
    .fill(PASSWORD);
  const { bodies, code } = await send(page);

  for (const body of bodies) {
    expect(body).not.toContain(snippet);
    expect(body).not.toContain(PASSWORD);
  }

  const visitor = await context.newPage();
  await visitor.goto(`/s/${code}`);
  const password = visitor.getByLabel("Password", { exact: true });
  await password.fill("definitely the wrong one");
  await visitor.getByRole("button", { name: "Unlock" }).click();
  await expect(visitor.getByText("Wrong password")).toBeVisible();
  await expect(visitor.getByText(snippet)).toBeHidden();

  await password.fill(PASSWORD);
  await visitor.getByRole("button", { name: "Unlock" }).click();
  await expect(visitor.getByText(snippet)).toBeVisible();
  await visitor.close();

  await deleteFromResult(page);
});
