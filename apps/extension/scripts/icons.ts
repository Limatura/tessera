/**
 * Renders the site's favicon (apps/web/public/favicon.svg) into the PNG sizes
 * a browser extension needs, so the toolbar shows the same mark as the site.
 * Run it again when the favicon changes: `pnpm --filter extension icons`.
 *
 * Chromium does the rasterising because it is already here for the end-to-end
 * suite, and draws the SVG exactly as the site's own tab does.
 */
import { readFileSync } from "node:fs";
import path from "node:path";

import { chromium } from "@playwright/test";

const ROOT = path.resolve(import.meta.dirname, "..");
const SOURCE = path.resolve(ROOT, "../web/public/favicon.svg");
const SIZES = [16, 32, 48, 96, 128];

const svg = readFileSync(SOURCE, "utf-8");
const browser = await chromium.launch({ channel: "chromium" });
try {
  for (const size of SIZES) {
    // oxlint-disable-next-line no-await-in-loop -- one page at a time is plenty for five images
    const page = await browser.newPage({
      viewport: { height: size, width: size },
    });
    // oxlint-disable-next-line no-await-in-loop -- as above
    await page.setContent(
      `<style>html,body{margin:0;background:transparent}svg{display:block;width:${size}px;height:${size}px}</style>${svg}`
    );
    // oxlint-disable-next-line no-await-in-loop -- as above
    await page.screenshot({
      omitBackground: true,
      path: path.join(ROOT, "public/icon", `${size}.png`),
    });
    // oxlint-disable-next-line no-await-in-loop -- as above
    await page.close();
  }
} finally {
  await browser.close();
}
