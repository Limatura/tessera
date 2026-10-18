/**
 * Everything the suite runs against, started once and torn down after:
 *
 * - a throwaway tessera-server with its own data directory, its first
 *   administrator's password set, so tests can change instance settings;
 * - a small site on two origins, for right-clicking media on a page and
 *   media served from somewhere else;
 * - a copy of the `--mode e2e` build whose manifest already grants both, as a
 *   test cannot answer the browser's permission prompt.
 *
 * What the tests need is handed over in `process.env`, which Playwright
 * passes on to its workers.
 */
import { spawn } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:http";
import type { Server } from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { crc32, deflateSync } from "node:zlib";

const ROOT = path.resolve(import.meta.dirname, "../..");
const REPO = path.resolve(ROOT, "../..");
const BUILD = path.join(ROOT, ".output/chrome-mv3-e2e");
const SERVER_BIN =
  process.env.TESSERA_SERVER_BIN ??
  path.join(REPO, "target/debug/tessera-server");
export const ADMIN_PASSWORD = "e2e administrator passphrase";

export default async function globalSetup() {
  if (!existsSync(BUILD)) {
    throw new Error(`no e2e build at ${BUILD}; run \`pnpm build:e2e\` first`);
  }
  if (!existsSync(SERVER_BIN)) {
    throw new Error(
      `no server binary at ${SERVER_BIN}; run \`pnpm build:web && cargo build --bin tessera-server\` from the repository root, or set TESSERA_SERVER_BIN`
    );
  }

  const scratch = mkdtempSync(path.join(tmpdir(), "tessera-extension-e2e-"));
  const [site, otherSite] = await Promise.all([serveSite(), serveSite()]);
  const tessera = await startTessera(scratch);
  const extension = prepareExtension(scratch, [tessera.origin, site.origin]);

  Object.assign(process.env, {
    TESSERA_E2E_ADMIN_PASSWORD: ADMIN_PASSWORD,
    TESSERA_E2E_EXTENSION: extension,
    TESSERA_E2E_OTHER_SITE: otherSite.origin,
    TESSERA_E2E_SERVER: tessera.origin,
    TESSERA_E2E_SITE: site.origin,
  });

  return () => {
    tessera.stop();
    site.server.close();
    otherSite.server.close();
    rmSync(scratch, { force: true, recursive: true });
  };
}

/** The unpacked build, copied, with host permissions for `origins` granted up front. */
function prepareExtension(scratch: string, origins: string[]): string {
  const target = path.join(scratch, "extension");
  cpSync(BUILD, target, { recursive: true });
  const manifestPath = path.join(target, "manifest.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf-8")) as Record<
    string,
    unknown
  >;
  manifest.host_permissions = origins.map((origin) => `${origin}/*`);
  writeFileSync(manifestPath, JSON.stringify(manifest));
  return target;
}

async function startTessera(scratch: string) {
  const port = await freePort();
  const dataDir = path.join(scratch, "data");
  const child = spawn(SERVER_BIN, [], {
    env: {
      ...process.env,
      TESSERA_AUTH__ADMIN_PASSWORD: ADMIN_PASSWORD,
      TESSERA_DATABASE__URL: `sqlite://${dataDir}/tessera.db?mode=rwc`,
      TESSERA_SERVER__DATA_DIR: dataDir,
      TESSERA_SERVER__HOST: "127.0.0.1",
      TESSERA_SERVER__PORT: String(port),
    },
    stdio: ["ignore", "ignore", "pipe"],
  });
  let log = "";
  child.stderr.on("data", (data: Buffer) => {
    log += data.toString();
  });
  const origin = `http://127.0.0.1:${port}`;
  await waitForHealth(
    origin,
    () => child.exitCode !== null,
    () => log
  );
  return { origin, stop: () => child.kill() };
}

const HEALTH_TIMEOUT_MS = 30_000;
const HEALTH_POLL_MS = 250;

async function waitForHealth(
  origin: string,
  exited: () => boolean,
  log: () => string
) {
  const deadline = Date.now() + HEALTH_TIMEOUT_MS;
  while (Date.now() < deadline) {
    if (exited()) {
      throw new Error(`tessera-server exited during start-up:\n${log()}`);
    }
    try {
      // oxlint-disable-next-line no-await-in-loop -- polling, one try at a time
      const response = await fetch(`${origin}/health`);
      if (response.ok) {
        return;
      }
    } catch {
      // Not listening yet.
    }
    // oxlint-disable-next-line no-await-in-loop -- polling interval
    await sleep(HEALTH_POLL_MS);
  }
  throw new Error(`tessera-server did not become healthy:\n${log()}`);
}

async function freePort(): Promise<number> {
  const { server, origin } = await listen(createServer());
  server.close();
  return Number(new URL(origin).port);
}

/**
 * A page with an image, a two-line paragraph and a link, and the image
 * itself. No CORS headers: fetched from another origin, it is refused.
 */
function serveSite() {
  const image = solidPng(16, 16, [200, 80, 20]);
  const server = createServer((request, response) => {
    if (request.url === "/cat.png") {
      response.writeHead(200, { "content-type": "image/png" });
      response.end(image);
      return;
    }
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(request.url?.startsWith("/blob/") ? CODE_VIEW : PAGE);
  });
  return listen(server);
}

const PAGE = `<!doctype html>
<title>Menu test page</title>
<img id="same" src="/cat.png" alt="">
<p id="text">first line of the poem<br>second line, kept apart</p>
<a id="link" href="https://example.com/some/article">an article</a>
<pre><code id="labelled" class="language-rs">fn main() {
    println!("hi");
}</code></pre>
<div class="highlight highlight-source-shell"><pre id="github">echo hi | grep h</pre></div>
<pre id="unlabelled">def greet(name):
    print(f"hello {name}")
</pre>`;

/**
 * A file view shaped like GitHub's: lines are monospace `<div>`s, with no
 * `<pre>` or `<code>`, under a clear monospace `<textarea>` holding the same
 * text, which is where a drag over the lines actually selects. Only the
 * address names the file, and the lines are ones the guess cannot place, so
 * only the address can say Rust. A comment box in an ordinary font sits
 * beside it, to stay plain text whatever it holds.
 */
const CODE_LINES = `    /// The instance it lives on.
    pub server: Url,
    /// Its retrieval code.
    pub code: String,`;
const CODE_FONT = "font-family: ui-monospace, SFMono-Regular, Menlo, monospace";
const CODE_VIEW = `<!doctype html>
<title>link.rs</title>
<p id="about">The link module reads what a person pastes.</p>
<div id="lines" style="${CODE_FONT}; white-space: pre">${CODE_LINES.split("\n")
  .map((line) => `<div>${line}</div>`)
  .join("")}</div>
<textarea id="overlay" readonly style="${CODE_FONT}; color: transparent">${CODE_LINES}</textarea>
<textarea id="comment" style="font-family: sans-serif">fn main() { let mut x = 1; }</textarea>`;

function listen(server: Server): Promise<{ server: Server; origin: string }> {
  // oxlint-disable-next-line promise/avoid-new -- `listen` only reports through a callback
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      const port = typeof address === "object" && address ? address.port : 0;
      resolve({ origin: `http://127.0.0.1:${port}`, server });
    });
  });
}

/** A PNG of one colour, so no binary fixture has to live in the repository. */
function solidPng(width: number, height: number, rgb: number[]): Buffer {
  const row = Buffer.concat([
    Buffer.from([0]),
    Buffer.from(Array.from({ length: width }, () => rgb).flat()),
  ]);
  const pixels = Buffer.concat(Array.from({ length: height }, () => row));
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  // 8-bit truecolour, default compression, filter and interlacing.
  header.set([8, 2, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(pixels)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function chunk(type: string, data: Buffer): Buffer {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}
