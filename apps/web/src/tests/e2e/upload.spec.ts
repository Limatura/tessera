import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";

// Home page smoke test: anonymous upload, get the code, fetch it in another tab, delete.
// Assertions use English strings; playwright.config.ts pins the locale to en via cookie.

const SHARE_PATH = /\/s\/(?<code>[A-Za-z0-9]+)$/u;
// In dev these are `@uiw_react-codemirror.js`, `@codemirror_lang-*.js` and `code-editor.tsx`;
// after a build, `code-editor-[hash].js`.
const CODE_EDITOR_REQUEST = /codemirror|code-editor/iu;

/**
 * Read the share code off the result screen. The displayed URL may use the
 * server's public_url, so take only the code and build the path.
 */
async function readShareCode(page: Page): Promise<string> {
  await expect(page.getByText("Ready to share")).toBeVisible();
  const url = await page.getByText(SHARE_PATH).textContent();
  const code = url?.match(SHARE_PATH)?.groups?.code;
  expect(code, `share URL not found in "${url}"`).toBeTruthy();
  return code as string;
}

/** Delete via the result screen, which also cleans up the test data. */
async function deleteFromResult(page: Page) {
  await page.getByRole("button", { exact: true, name: "Delete" }).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog.getByText("Delete this share?")).toBeVisible();
  await dialog.getByRole("button", { exact: true, name: "Delete" }).click();
  await expect(page.getByText("Ready to share")).toBeHidden();
}

test("the home page does not load CodeMirror before the code tab is used", async ({
  page,
}) => {
  const requested: string[] = [];
  page.on("request", (request) => {
    requested.push(request.url());
  });

  await page.goto("/");
  await expect(page.getByText("Share something")).toBeVisible();
  await page.waitForLoadState("networkidle");

  expect(requested.filter((url) => CODE_EDITOR_REQUEST.test(url))).toEqual([]);

  const codeTab = page.getByRole("tab", { name: "Code" });
  const editorRequest = page.waitForRequest(CODE_EDITOR_REQUEST);
  await codeTab.hover();
  await editorRequest;

  await codeTab.click();
  await expect(page.locator(".cm-content")).toBeVisible();
});

test("once a language is detected the editor loads its grammar and highlights", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("tab", { name: "Code" }).click();
  const editor = page.locator(".cm-content");
  await editor.click();
  // `let mut` is a Rust-only rule in detect-language; one match settles it.
  await page.keyboard.type("fn main() { let mut total = 0; }");

  await expect(page.getByText("auto-detected")).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Language" })).toHaveText(
    /rust/iu
  );
  // As plain text each line is a single text node; once the grammar loads, tokens get spans.
  // Move the cursor off `}` first, or bracket matching's highlight span would
  // pass this assertion by itself.
  await page.keyboard.press("Home");
  await expect(editor.locator(".cm-line span").first()).toBeVisible();
});

test("an uploaded snippet can be retrieved with its code", async ({
  context,
  page,
}) => {
  const snippet = `tessera e2e snippet ${Date.now()}`;

  await page.goto("/");
  await page.getByRole("tab", { name: "Code" }).click();
  const editor = page.locator(".cm-content");
  await editor.click();
  await page.keyboard.type(snippet);
  await page.getByRole("button", { name: "Send snippet" }).click();

  const code = await readShareCode(page);

  const visitor = await context.newPage();
  await visitor.goto(`/s/${code}`);
  await expect(visitor.getByText(snippet)).toBeVisible();
  await visitor.close();

  await deleteFromResult(page);
});

test("an uploaded file can be retrieved with its code", async ({
  context,
  page,
}) => {
  const filename = `tessera-e2e-${Date.now()}.txt`;

  await page.goto("/");
  await page.locator('input[type="file"]').setInputFiles({
    buffer: Buffer.from("hello from the tessera e2e suite\n"),
    mimeType: "text/plain",
    name: filename,
  });

  const code = await readShareCode(page);

  const visitor = await context.newPage();
  await visitor.goto(`/s/${code}`);
  await expect(visitor.getByText(filename)).toBeVisible();
  await expect(visitor.getByRole("button", { name: "Download" })).toBeVisible();
  await visitor.close();

  await deleteFromResult(page);
});
