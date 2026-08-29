import { writeFile } from "node:fs/promises";

import { render } from "takumi-js";
import { googleFonts } from "takumi-js/helpers";

import FileOg from "./file";
import LinkOg from "./link";
import SiteOg from "./site";
import SnippetOg from "./snippet";

const fonts = googleFonts([{ name: "Inter", weight: "400..800" }]);

async function run() {
  const jobs: [string, () => Promise<JSX.Element> | JSX.Element][] = [
    ["og-site", () => <SiteOg />],
    ["og-file-single", () => <FileOg variant="single" />],
    [
      "og-file-multi",
      () => (
        <FileOg
          variant="multi"
          filename="holiday.jpg"
          extraFiles={5}
          sizeLabel="86 MB"
        />
      ),
    ],
    [
      "og-file-folder",
      () => (
        <FileOg
          variant="folder"
          folderName="vacation-photos"
          fileCount={24}
          sizeLabel="312 MB"
        />
      ),
    ],
    ["og-link", () => <LinkOg />],
    [
      "og-link-long",
      () => (
        <LinkOg url="https://example.com/blog/2026/why-anonymous-file-sharing-still-matters-in-the-age-of-cloud-storage" />
      ),
    ],
    [
      "og-file-long-name",
      () => (
        <FileOg
          variant="single"
          filename="Q3-2026-finance-and-operations-report-final-v3.pdf"
        />
      ),
    ],
    ["og-snippet", () => SnippetOg({})],
    [
      "og-snippet-long",
      () =>
        SnippetOg({
          title: "very-long-service-with-a-descriptive-name.ts",
          code: `import { createHighlighterForAllSupportedLanguagesAndThemesEverMade } from "some-extremely-long-package-name/deep/nested/path";

export function withAuth(handler: Handler) {
  return async (req: Request) => {
    const token = req.headers.get("authorization");
    if (!token) return new Response("Unauthorized", { status: 401 });

    const session = await verifySession(token);
    if (!session) return new Response("Forbidden", { status: 403 });

    return handler(req, session);
  };
}

export function withLogging(handler: Handler) {
  return async (req: Request) => {
    console.log("incoming", req.method, req.url);
    return handler(req);
  };
}`,
        }),
    ],
  ];

  await Promise.all(
    jobs.map(async ([name, build]) => {
      const el = await build();
      const png = await render(el, { width: 1200, height: 630, fonts });
      await writeFile(`./out-${name}.png`, png);
      console.log(`wrote out-${name}.png`);
    })
  );
}

try {
  await run();
} catch (error) {
  console.error(error);
  process.exit(1);
}
