# End-to-end tests

The tests are in this directory, configured in `apps/web/playwright.config.ts`. The approach follows [nelsonlaidev/nelsonlai.dev][ref], but this project is a Rust backend with SQLite, so a few parts cannot be copied as they are; they are marked below.

The suite needs a backend with a known admin password and end-to-end encryption switched on (it is off by default). The simplest is a throwaway one on its own port, with vite's proxy pointed at it through `TESSERA_API_ORIGIN`, so the backend you work with is left alone:

```bash
cargo build -p tessera-server      # a debug build is enough
TESSERA_SERVER__PORT=8788 TESSERA_SERVER__DATA_DIR=/tmp/tessera-e2e \
  TESSERA_DATABASE__URL=sqlite:///tmp/tessera-e2e/tessera.db \
  TESSERA_AUTH__ADMIN_PASSWORD='E2e-Tessera-9vQx-Lm4p' \
  TESSERA_FEATURES__ENCRYPTION=true target/debug/tessera-server &
TESSERA_API_ORIGIN=http://127.0.0.1:8788 E2E_ADMIN_PASSWORD='E2e-Tessera-9vQx-Lm4p' \
  pnpm --filter web test:e2e       # Playwright starts vite dev (:3001), or reuses one already running
```

`E2E_ADMIN_PASSWORD` must match `TESSERA_AUTH__ADMIN_PASSWORD`; without it the setup project fails with a message saying so. `E2E_ADMIN_USERNAME` defaults to `admin`, the server's default. A configured admin password is not marked "must change", which a generated one would be. The password has to pass the instance's password policy, so do not shorten it to something weak.

CI runs these in the `e2e` job of `.github/workflows/web.yml` on GitHub-hosted runners, whenever `web` is affected. It starts the backend with the same admin password and encryption switch as above.

[ref]: https://github.com/nelsonlaidev/nelsonlai.dev

## Why Playwright

Plain Playwright, not Vitest Browser Mode. These tests walk a whole flow (upload, get a code, open the link, see the content), and Playwright's auto-waiting, trace viewer and multi-browser support are far more mature for that; Browser Mode leans towards components.

## Why they are kept apart from the unit tests

Unit tests are colocated (`lib/upload.test.ts` sits beside `lib/upload.ts`), so the test is next door when the code changes. e2e tests cannot be:

- They use Playwright's `test` and `expect`, not vitest's. vitest fails while collecting as soon as it loads one, so `vitest.config.ts` explicitly excludes this directory.
- They need a running server and a browser, nothing like what a pure-function test needs.

**The extension is `.spec.ts`** (not `.test.ts`) as a second safeguard: even if the exclude breaks one day, `.spec.ts` is not in vitest's include.

## Projects

`playwright.config.ts` defines three:

- `setup` runs `auth.setup.ts`: `POST /api/v1/auth/login` as the admin, then saves the cookies (session and locale) to `playwright/.auth/admin.json`, which is gitignored
- `authenticated` depends on `setup`, loads that file as `storageState`, and runs only tests tagged `@authenticated`
- `anonymous` runs every other test with no session

Signed-in tests go in a `test.describe(..., { tag: "@authenticated" }, ...)` block in whatever spec they belong to; there is no separate directory for them. A test that logs out must sign in on its own first, as the form test in `auth.spec.ts` does, or it would revoke the session the other signed-in tests share.

## The tests

`upload.spec.ts`, all anonymous:

- The home page does not load CodeMirror before the code tab is touched. Guards the lazy loading in `upload-panel.tsx`; it fails if someone turns `CodeUploadForm` back into a static import
- Once a language is detected, the editor loads its grammar and highlights. Guards the dynamic grammar loading in `code-upload-form.tsx`
- Upload a snippet → code → retrieve it in another tab → delete
- Upload a file → code → retrieve it in another tab → delete

`auth.spec.ts`:

- Anonymous: `/admin` sends the visitor to `/login?redirect=…`; a wrong password shows the error and stays on the page; signing in through the form lands on the admin dashboard, and Logout ends that session
- `@authenticated`: `/admin` opens on the dashboard; the user menu leads to the admin console; `GET /api/v1/auth/session` answers for the admin

`encrypted-share.spec.ts`, anonymous (needs `TESSERA_FEATURES__ENCRYPTION=true`):

- A link-keyed snippet: the result URL carries the key after `#`, no request body sent to `/api/` contains the plaintext, the full link opens it, and the link without the fragment shows "This link is incomplete"
- A password-keyed snippet: no request body contains the plaintext or the password, a wrong password is refused, the right one opens it

Assertions use English strings; `playwright.config.ts` pins the locale to `en` with the `PARAGLIDE_LOCALE` cookie. Prefer semantic queries such as `getByRole` and `getByLabel`, and add a `data-testid` only when those cannot find it.

## Where this project differs from the reference repo

**Getting a backend up is the main difference.** The reference repo is a single Next.js app, and one `webServer` line starts everything. Here the two halves are separate: vite dev (:3001) proxies `/api` to the Rust server (:12923), so a backend has to be running before the tests. Two ways:

- `pnpm docker:up` (the whole compose setup, frontend included; test :12923 directly, no vite)
- or `pnpm server:dev` locally and let Playwright's `webServer` start vite

**Do not copy their auth setup.** They write a session row straight into the database with drizzle and sign the cookie themselves, which works because front and back end are one JS project sharing a database client. Here the database belongs to Rust, and the frontend has (and should have) no way to write to it. To test a signed-in state, go through the API: `POST /api/v1/auth/login` for the cookie, saved as `storageState`.

**Clean up test data yourself.** They have a `global.teardown.ts` that deletes what the tests created. Here each upload test ends by pressing "Delete" on the result screen; the management token is only on that screen, and it tests deletion along the way. The cost is that a share from a test that failed midway is not deleted and has to expire by itself (7 days by default). For something cleaner, catch the token from the upload response and call `DELETE /api/v1/shares/{code}` in `afterEach`.

**Authenticated and anonymous projects.** Like the reference repo, a `setup` project prepares the signed-in state once and the `authenticated` project depends on it. Where they split by file, this suite splits by the `@authenticated` tag, so signed-in and signed-out checks of one feature can sit in one spec.

> Note: Playwright's chromium needs system libraries such as `libnspr4`, `libnss3` and `libcups2`. If they are missing, run `playwright install --with-deps chromium` first. On a distribution that list does not cover (Ubuntu 26.04, for example), find what is missing with `ldd ~/.cache/ms-playwright/chromium-*/chrome-linux64/chrome | grep "not found"` and install it with apt.
>
> Keep `channel: "chromium"` in the config: the default chrome-headless-shell computes grid rows under `container-type: inline-size` as 0 high, the whole `CardHeader` collapses, and every card title is judged hidden. For the same reason CI cannot install with `--only-shell`.
