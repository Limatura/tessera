import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";

// 首頁冒煙測試:匿名上傳 → 拿到取件碼 → 用另一個分頁取回 → 刪掉。
// 斷言用英文字串,語系在 playwright.config.ts 用 cookie 釘成 en。

const SHARE_PATH = /\/s\/(?<code>[A-Za-z0-9]+)$/u;
// dev 下是 `@uiw_react-codemirror.js`、`@codemirror_lang-*.js`、`code-editor.tsx`;
// build 後是 `code-editor-[hash].js`。
const CODE_EDITOR_REQUEST = /codemirror|code-editor/iu;

/** 從結果畫面讀出取件碼。顯示的網址可能是伺服器的 public_url,只取 code 自己組路徑。 */
async function readShareCode(page: Page): Promise<string> {
  await expect(page.getByText("Ready to share")).toBeVisible();
  const url = await page.getByText(SHARE_PATH).textContent();
  const code = url?.match(SHARE_PATH)?.groups?.code;
  expect(code, `share URL not found in "${url}"`).toBeTruthy();
  return code as string;
}

/** 走結果畫面上的刪除,順便把測試資料清掉。 */
async function deleteFromResult(page: Page) {
  await page.getByRole("button", { exact: true, name: "Delete" }).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog.getByText("Delete this share?")).toBeVisible();
  await dialog.getByRole("button", { exact: true, name: "Delete" }).click();
  await expect(page.getByText("Ready to share")).toBeHidden();
}

test("首頁在用到程式碼分頁之前不載入 CodeMirror", async ({ page }) => {
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

test("偵測到語言後編輯器會載入語法包並上色", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("tab", { name: "Code" }).click();
  const editor = page.locator(".cm-content");
  await editor.click();
  // `let mut` 是 detect-language 裡 Rust 的獨有規則,一條就定案。
  await page.keyboard.type("fn main() { let mut total = 0; }");

  await expect(page.getByText("auto-detected")).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Language" })).toHaveText(
    /rust/iu
  );
  // 純文字時每行只有一個 text node;語法包載進來才會切出 token 的 span。
  // 游標先移開 `}`,不然括號配對的高亮也是 span,會讓這條斷言白過。
  await page.keyboard.press("Home");
  await expect(editor.locator(".cm-line span").first()).toBeVisible();
});

test("上傳程式碼片段後可以用取件碼取回", async ({ context, page }) => {
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

test("上傳檔案後可以用取件碼取回", async ({ context, page }) => {
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
