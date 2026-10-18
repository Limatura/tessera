import type { Page } from "@playwright/test";

import { expect, test } from "./fixtures";
import { describeShare, env, readNote, setEncryption } from "./instance";

test.describe("right-click menu", () => {
  test.beforeEach(async ({ extension, page }) => {
    // Unsealed, so what arrived can be read back from the API.
    await setEncryption(false);
    await extension.useTestServer();
    await page.goto(`${env().site}/page.html`);
  });

  test("an image on the page uploads as a file named after its URL", async ({
    extension,
    page,
  }) => {
    const job = await extension.menuClick(page, {
      menuItemId: "upload-image",
      srcUrl: `${env().site}/cat.png`,
    });
    expect(job, job.error).toMatchObject({ name: "cat.png", state: "done" });
    const share = await describeShare(job.link ?? "");
    expect(share?.kind).toBe("file");
    expect(share?.files[0]?.filename).toBe("cat.png");
    expect(share?.files[0]?.size_bytes).toBeGreaterThan(0);
  });

  test("an image another origin will not share fails, saying what permission would help", async ({
    extension,
    page,
  }) => {
    const job = await extension.menuClick(page, {
      menuItemId: "upload-image",
      srcUrl: `${env().otherSite}/cat.png`,
    });
    expect(job.state).toBe("failed");
    expect(job.error).toContain("Fetch media from any site");
  });

  test("a selection keeps its line breaks and is titled after the page", async ({
    extension,
    page,
  }) => {
    await select(page, "#text");
    const job = await extension.menuClick(page, {
      menuItemId: "upload-selection",
      // What Chrome itself reports: the line break already gone.
      selectionText: "first line of the poem second line, kept apart",
    });
    expect(job, job.error).toMatchObject({ kind: "note", state: "done" });
    // Prose is never guessed at: it stays plain text.
    expect(await describeShare(job.link ?? "")).toMatchObject({
      kind: "text",
      language: null,
      title: "Menu test page",
    });

    expect(await readNote(job.link ?? "")).toBe(
      "first line of the poem\nsecond line, kept apart"
    );
  });

  test.describe("code", () => {
    for (const [selector, expected, how] of [
      ["#labelled", "rust", "named by its class, in another tool's spelling"],
      ["#github", "shellscript", "named by GitHub's class on a wrapper"],
      ["#unlabelled", "python", "guessed, since it sits in a code block"],
    ] as const) {
      test(`a selection in a code block carries its language: ${how}`, async ({
        extension,
        page,
      }) => {
        await select(page, selector);
        const job = await extension.menuClick(page, {
          menuItemId: "upload-selection",
        });
        expect(job.state, job.error).toBe("done");
        const share = await describeShare(job.link ?? "");
        expect(share?.language).toBe(expected);
      });
    }
  });

  test.describe("a file view without <pre>, as GitHub draws one", () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(`${env().site}/blob/main/src/link.rs`);
    });

    test("monospace lines are code, in the language the address names", async ({
      extension,
      page,
    }) => {
      await select(page, "#lines");
      const job = await extension.menuClick(page, {
        menuItemId: "upload-selection",
      });
      expect(job.state, job.error).toBe("done");
      const share = await describeShare(job.link ?? "");
      expect(share?.language).toBe("rust");
    });

    test("a selection in its clear text field, where GitHub's actually is, keeps its line breaks", async ({
      extension,
      page,
    }) => {
      await selectInField(page, "#overlay");
      const job = await extension.menuClick(page, {
        menuItemId: "upload-selection",
        // What Chrome reports for it: the line breaks gone.
        selectionText: "/// The instance it lives on. pub server: Url,",
      });
      expect(job.state, job.error).toBe("done");
      const share = await describeShare(job.link ?? "");
      expect(share?.language).toBe("rust");
      expect(await readNote(job.link ?? "")).toContain(
        "lives on.\n    pub server: Url,"
      );
    });

    test("a comment box in an ordinary font stays plain text, even holding code", async ({
      extension,
      page,
    }) => {
      await selectInField(page, "#comment");
      const job = await extension.menuClick(page, {
        menuItemId: "upload-selection",
      });
      expect(job.state, job.error).toBe("done");
      const share = await describeShare(job.link ?? "");
      expect(share?.language).toBeNull();
    });

    test("prose on the same page stays plain text", async ({
      extension,
      page,
    }) => {
      await select(page, "#about");
      const job = await extension.menuClick(page, {
        menuItemId: "upload-selection",
      });
      expect(job.state, job.error).toBe("done");
      const share = await describeShare(job.link ?? "");
      expect(share?.language).toBeNull();
    });
  });

  test("a link and the page itself become link shares", async ({
    extension,
    page,
  }) => {
    const link = await extension.menuClick(page, {
      linkUrl: "https://example.com/some/article",
      menuItemId: "share-link",
    });
    expect(link.state, link.error).toBe("done");
    const linkShare = await describeShare(link.link ?? "");
    expect(linkShare?.kind).toBe("link");

    const self = await extension.menuClick(page, {
      menuItemId: "share-page",
      pageUrl: page.url(),
    });
    expect(self.state, self.error).toBe("done");
    const selfShare = await describeShare(self.link ?? "");
    expect(selfShare?.kind).toBe("link");
  });

  test("the file a link points at is downloaded and uploaded", async ({
    extension,
    page,
  }) => {
    const job = await extension.menuClick(page, {
      linkUrl: `${env().site}/cat.png`,
      menuItemId: "upload-linked-file",
    });
    expect(job.state, job.error).toBe("done");
    const share = await describeShare(job.link ?? "");
    expect(share?.files[0]?.filename).toBe("cat.png");
  });
});

/** Selects everything inside the element `selector` finds, as a drag would. */
async function select(page: Page, selector: string) {
  await page.evaluate((target) => {
    const range = document.createRange();
    range.selectNodeContents(document.querySelector(target) as Node);
    getSelection()?.removeAllRanges();
    getSelection()?.addRange(range);
  }, selector);
}

/** Focuses a text field and selects all it holds, as a drag in it would. */
async function selectInField(page: Page, selector: string) {
  await page.locator(selector).evaluate((field) => {
    const input = field as HTMLTextAreaElement;
    input.focus();
    input.setSelectionRange(0, input.value.length);
  });
}
