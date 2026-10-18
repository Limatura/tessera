/**
 * A fresh Chromium profile with the extension loaded, for every test, so no
 * test sees another's settings, sign-in or uploads.
 */
/* oxlint-disable react-hooks/rules-of-hooks -- Playwright names a fixture's callback `use`; it is not React's */
import { expect, test as base, chromium } from "@playwright/test";
import type { BrowserContext, Page, Worker } from "@playwright/test";
import type { browser } from "wxt/browser";

import { env } from "./instance";

export { expect } from "@playwright/test";

/**
 * The extension API inside an `evaluate` callback, which runs in the
 * extension's own worker and is typed here only for the compiler.
 */
type ExtensionApi = typeof browser;

/** A job as `lib/jobs.ts` stores it; only what tests read. */
export interface StoredJob {
  id: string;
  name: string;
  kind: "file" | "note" | "link";
  state: "preparing" | "uploading" | "done" | "failed" | "cancelled";
  link?: string;
  error?: string;
}

/** What a right-click hands the menu handler. */
export interface MenuClick {
  menuItemId: string;
  srcUrl?: string;
  linkUrl?: string;
  pageUrl?: string;
  selectionText?: string;
}

export interface Extension {
  id: string;
  /** An extension page in a new tab, brought to the front. */
  open: (name: "popup" | "options") => Promise<Page>;
  /** Points the extension at the test instance without going through the UI. */
  useTestServer: () => Promise<void>;
  /** One key of the extension's storage, as it is saved. */
  stored: (area: "local" | "sync", key: string) => Promise<unknown>;
  jobs: () => Promise<StoredJob[]>;
  /** Waits for a job other than those in `before` to finish, and returns it. */
  nextJob: (before: StoredJob[]) => Promise<StoredJob>;
  /** Right-clicks a menu item on `page`, as the browser would report it. */
  menuClick: (page: Page, click: MenuClick) => Promise<StoredJob>;
}

const FINISHED = new Set(["done", "failed", "cancelled"]);

export const test = base.extend<{
  context: BrowserContext;
  worker: Worker;
  extension: Extension;
}>({
  // oxlint-disable-next-line no-empty-pattern -- Playwright reads fixture dependencies from the pattern
  context: async ({}, use) => {
    const { extension } = env();
    // An empty profile path is a fresh temporary profile.
    const context = await chromium.launchPersistentContext("", {
      args: [
        `--disable-extensions-except=${extension}`,
        `--load-extension=${extension}`,
        "--lang=en-US",
      ],
      // Full Chromium, like apps/web: the headless shell cannot load extensions.
      channel: "chromium",
      headless: true,
      locale: "en-US",
    });
    await use(context);
    await context.close();
  },

  extension: async ({ context, worker }, use) => {
    const id = new URL(worker.url()).host;
    const jobs = () =>
      worker.evaluate(async () => {
        const { chrome } = globalThis as unknown as { chrome: ExtensionApi };
        const all = await chrome.storage.session.get(null);
        return Object.entries(all)
          .filter(([key]) => key.startsWith("job:"))
          .map(([, value]) => value as StoredJob);
      });
    const nextJob = async (before: StoredJob[]) => {
      const seen = new Set(before.map((job) => job.id));
      let found: StoredJob | undefined;
      await expect
        .poll(
          async () => {
            const current = await jobs();
            found = current.find(
              (job) => !seen.has(job.id) && FINISHED.has(job.state)
            );
            return found !== undefined;
          },
          { timeout: 30_000 }
        )
        .toBe(true);
      return found as StoredJob;
    };

    await use({
      id,
      jobs,
      menuClick: async (page, click) => {
        const before = await jobs();
        await worker.evaluate(
          async ({ click: info, url }) => {
            const { chrome } = globalThis as unknown as {
              chrome: ExtensionApi;
            };
            const [tab] = await chrome.tabs.query({ url });
            // Exposed by the `--mode e2e` build only; see entrypoints/background.ts.
            const { tesseraMenuClick } = globalThis as unknown as {
              tesseraMenuClick: (info: unknown, tab: unknown) => Promise<void>;
            };
            await tesseraMenuClick({ frameId: 0, ...info }, tab);
          },
          { click, url: page.url() }
        );
        return nextJob(before);
      },
      nextJob,
      open: async (name) => {
        const page = await context.newPage();
        await page.goto(`chrome-extension://${id}/${name}.html`);
        // Headless Chromium pauses animation frames in background tabs, and
        // Playwright's checks that an element is stable wait on them.
        await page.bringToFront();
        return page;
      },
      stored: (area, key) =>
        worker.evaluate(
          async ({ area: where, key: name }) => {
            const { chrome } = globalThis as unknown as {
              chrome: ExtensionApi;
            };
            const values = await chrome.storage[where].get(name);
            return values[name];
          },
          { area, key }
        ),
      useTestServer: async () => {
        await worker.evaluate(async (server) => {
          const { chrome } = globalThis as unknown as { chrome: ExtensionApi };
          await chrome.storage.local.set({ server });
        }, env().server);
      },
    });
  },

  worker: async ({ context }, use) => {
    const worker =
      context.serviceWorkers()[0] ??
      (await context.waitForEvent("serviceworker"));
    await use(worker);
  },
});
