# End-to-end tests

The extension, loaded into Chromium, against a real tessera-server.

```bash
pnpm build:web                          # from the repository root: the server serves this bundle
cargo build --bin tessera-server        # the binary the suite starts
pnpm --filter extension test:e2e        # builds with --mode e2e, then runs Playwright
```

Unlike `apps/web`'s suite, nothing has to be running first. `global-setup.ts` starts everything and stops it afterwards:

- a throwaway server on a free port, with its own data directory and the first administrator's password set (`TESSERA_AUTH__ADMIN_PASSWORD`), so tests can switch instance settings such as encryption;
- a small site on two origins: a page with an image, a two-line paragraph, a link and three code blocks (marked by class, marked GitHub's way on a wrapper, unmarked), and the same image on an origin that sends no CORS headers;
- a copy of the `--mode e2e` build whose manifest grants both origins, because a test cannot answer the browser's permission prompt.

Set `TESSERA_SERVER_BIN` to test another binary (a release build, say).

## What is only in the e2e build

No browser API can click a native context menu, so `--mode e2e` exposes the click handler as `globalThis.tesseraMenuClick`, and `extension.menuClick()` calls it with what Chrome would report. Everything after the click is real: fetching the media, reading the selection from the page, uploading. The condition is a build-time constant, so other builds do not carry the line.

## Notes

- Every test gets a fresh browser profile (`fixtures.ts`): no settings, sign-in or uploads carry over.
- One worker, in order: encryption is one switch for the whole instance, and tests flip it.
- `extension.open()` brings the page to the front. Headless Chromium pauses animation frames in background tabs, and Playwright waits on them to decide an element is stable, so a click on a page behind another one stalls.
- Firefox is not covered: Playwright cannot load an extension into Firefox.
- Chromium needs the same system libraries as `apps/web`'s suite; see its README.
