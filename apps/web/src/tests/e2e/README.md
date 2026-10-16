# End-to-end tests

The home page smoke tests are in `upload.spec.ts`, configured in `apps/web/playwright.config.ts`. The approach follows [nelsonlaidev/nelsonlai.dev][ref], but this project is a Rust backend with SQLite, so a few parts cannot be copied as they are; they are marked below.

```bash
pnpm server:dev                    # start the backend first (:12923)
pnpm --filter web test:e2e         # Playwright starts vite dev (:3001), or reuses one already running
```

To leave the backend you are working with alone, start a throwaway one and point vite's proxy at it with `TESSERA_API_ORIGIN`:

```bash
TESSERA_SERVER__PORT=8788 TESSERA_SERVER__DATA_DIR=/tmp/tessera-e2e \
  TESSERA_DATABASE__URL=sqlite:///tmp/tessera-e2e/tessera.db target/debug/tessera-server &
TESSERA_API_ORIGIN=http://127.0.0.1:8788 pnpm --filter web test:e2e
```

The CI job is written in `.github/workflows/web.yml` (`e2e`) but **commented out**: the self-hosted runner it was written for lacked the system libraries Chromium needs. The comment above it lists what to change to turn it on with GitHub-hosted runners.

[ref]: https://github.com/nelsonlaidev/nelsonlai.dev

## Why Playwright

Plain Playwright, not Vitest Browser Mode. These tests walk a whole flow (upload, get a code, open the link, see the content), and Playwright's auto-waiting, trace viewer and multi-browser support are far more mature for that; Browser Mode leans towards components.

## Why they are kept apart from the unit tests

Unit tests are colocated (`lib/upload.test.ts` sits beside `lib/upload.ts`), so the test is next door when the code changes. e2e tests cannot be:

- They use Playwright's `test` and `expect`, not vitest's. vitest fails while collecting as soon as it loads one, so `vitest.config.ts` explicitly excludes this directory.
- They need a running server and a browser, nothing like what a pure-function test needs.

**The extension is `.spec.ts`** (not `.test.ts`) as a second safeguard: even if the exclude breaks one day, `.spec.ts` is not in vitest's include.

## The tests

`upload.spec.ts`, all anonymous:

- The home page does not load CodeMirror before the code tab is touched. Guards the lazy loading in `upload-panel.tsx`; it fails if someone turns `CodeUploadForm` back into a static import
- Once a language is detected, the editor loads its grammar and highlights. Guards the dynamic grammar loading in `code-upload-form.tsx`
- Upload a snippet → code → retrieve it in another tab → delete
- Upload a file → code → retrieve it in another tab → delete

Assertions use English strings; `playwright.config.ts` pins the locale to `en` with the `PARAGLIDE_LOCALE` cookie. Prefer semantic queries such as `getByRole` and `getByLabel`, and add a `data-testid` only when those cannot find it.

## Where this project differs from the reference repo

**Getting a backend up is the main difference.** The reference repo is a single Next.js app, and one `webServer` line starts everything. Here the two halves are separate: vite dev (:3001) proxies `/api` to the Rust server (:12923), so a backend has to be running before the tests. Two ways:

- `pnpm docker:up` (the whole compose setup, frontend included; test :12923 directly, no vite)
- or `pnpm server:dev` locally and let Playwright's `webServer` start vite

**Do not copy their auth setup.** They write a session row straight into the database with drizzle and sign the cookie themselves, which works because front and back end are one JS project sharing a database client. Here the database belongs to Rust, and the frontend has (and should have) no way to write to it. To test a signed-in state, go through the API: `POST /api/v1/auth/login` for the cookie, saved as `storageState`.

**Clean up test data yourself.** They have a `global.teardown.ts` that deletes what the tests created. Here each upload test ends by pressing "Delete" on the result screen; the management token is only on that screen, and it tests deletion along the way. The cost is that a share from a test that failed midway is not deleted and has to expire by itself (7 days by default). For something cleaner, catch the token from the upload response and call `DELETE /api/v1/shares/{code}` in `afterEach`.

**Separate authenticated and unauthenticated projects?** They use `projects` with `dependencies` to prepare a signed-in state once for many tests. Most of this app (uploading, sharing, browsing) works anonymously, so one project is enough for now; split it when several tests actually need to be signed in.

> Note: Playwright's chromium needs system libraries such as `libnspr4`, `libnss3` and `libcups2`. If they are missing, run `playwright install --with-deps chromium` first. On a distribution that list does not cover (Ubuntu 26.04, for example), find what is missing with `ldd ~/.cache/ms-playwright/chromium-*/chrome-linux64/chrome | grep "not found"` and install it with apt.
>
> Keep `channel: "chromium"` in the config: the default chrome-headless-shell computes grid rows under `container-type: inline-size` as 0 high, the whole `CardHeader` collapses, and every card title is judged hidden. For the same reason CI cannot install with `--only-shell`.
