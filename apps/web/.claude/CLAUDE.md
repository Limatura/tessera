# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**@apps/web** is the React frontend for Tessera, an anonymous file courier service. The app supports three distinct flows:

1. **Anonymous Mode** (home page `/`) — Users can upload files, code snippets, or links without authentication. They receive a retrieval code, and the payload self-destructs after expiration or reaching the retrieval limit. This is the primary use case.

2. **User Mode** (authenticated `/login/`) — Registered users can log in (email/password or Google OAuth) to access account features like upload history, saved shares, and higher limits. Sign-up is also available.

3. **Admin panel** (`/admin/*`) — The operator console: server configuration (accounts, sign-in providers, mail, appearance, uploads, storage) and monitoring (instance stats, storage trend, recent shares). It used to be a separate app (`apps/admin`); it is now a route subtree of this SPA so moving between the site and the panel is a client-side navigation, not a page load. See **Admin Panel** below.

The app is embedded in the Rust backend binary and served from it — there is no separate deployment layer. All business logic lives in the backend API; this frontend is purely presentation and orchestration.

## Tech Stack

- **React 19+** with TanStack Router v1
- **Vite** for bundling and dev server (port 3001)
- **TypeScript** for type safety
- **Tailwind CSS** + **shadcn/ui** (via @tessera/ui) for styling
- **Paraglide** for i18n (no route-prefixed locales; uses cookies + Accept-Language)
- **CodeMirror** with multi-language support for syntax highlighting
- **Zustand** for state management
- **React Hook Form** + **Zod** for form validation
- **QRCode** for share link generation
- **Recharts** (via `@tessera/ui/components/chart`) — admin panel only; route-level code splitting keeps it out of the site's bundle

### Related Packages

- `@tessera/ui` — shared shadcn/ui component library
- `@tessera/i18n` — i18n utilities and message definitions

## Project Structure

```
src/
├── routes/                 # TanStack Router file-based routing
│   ├── __root.tsx         # Root layout (theme, appearance, locale, toaster) — shared by site and admin
│   ├── _site/             # Pathless layout: everything with the site Header
│   │   ├── route.tsx      # Renders <Header /> + <Outlet />
│   │   ├── index.tsx      # Anonymous upload mode (no auth required)
│   │   ├── login/         # Auth flow (login + sign-up)
│   │   ├── s/$code/       # Share link retrieval (unauthenticated access)
│   │   └── -components/   # Private route-local components (prefix with -)
│   └── admin/             # The operator console at /admin — see "Admin Panel"
│       └── route.tsx      # Admin layout: requireAuth + AdminShell; pages set `staticData.title`
├── components/            # Shared app components
│   ├── header.tsx         # Site nav with locale selector and theme toggle
│   └── admin/             # AdminShell, sidebar, nav, pager — admin only
├── hooks/                 # Query hooks; admin ones under hooks/admin/
├── lib/                   # Utilities; admin ones under lib/admin/
│   ├── upload.ts          # Upload orchestration
│   ├── locale.ts          # Locale subscription + document application
│   └── ...
├── paraglide/             # Generated i18n files (auto-created by Paraglide)
└── index.css              # Tailwind entry point
```

The site's Header lives in the `_site` layout, not in `__root`, because the admin panel has its own sidebar frame. A new site page goes under `routes/_site/`; a new admin page under `routes/admin/`.

**Convention:** Components local to a route use the `-components/` prefix (leading dash) so TanStack Router ignores them when scanning for route files.

### Route Roles

- **`/` (Anonymous Mode)** — The main interface for all users. Three upload tabs: File, Code (syntax highlighting), Link. No authentication gate.
- **`/login/`** — Login and sign-up form. Supports email/password and Google OAuth. Linked from the sign-up prompt on the home page.
- **`/s/$code/`** — Public share link retrieval. Any visitor can access this with a valid code; used to download or retrieve the shared content.
- **`/admin/*`** — The admin panel. Every page gates itself on a permission (see Admin Panel).

## Development

### Common Commands

```bash
# From monorepo root
pnpm dev:web              # Start dev server + watch (localhost:3001; admin at /admin)
pnpm check                # Format + lint (Vite+ `vp check`, Ultracite rules)
pnpm fix                  # Auto-fix linting and formatting issues

# From apps/web
pnpm dev                  # Start dev server
pnpm build                # Vite production build
pnpm check-types          # Type check + build (catches routing issues)
```

### Key Concepts

**TanStack Router (v1)**

- Routes are file-based in `src/routes/`. Filename structure determines routing:
  - `index.tsx` → route segment
  - `__root.tsx` → root layout
  - `route.tsx` → layout for its directory
  - `_name/` → pathless layout (groups routes under a layout without adding a URL segment)
  - `$param` → dynamic segment (`:param`)
  - Files starting with `-` are route-local helpers, not routes
- Define routes with `createFileRoute("/id")({ ... })`, never code-based `createRoute` + `getParentRoute`: the plugin wires the parent, and a hand-written `getParentRoute` points at the wrong layout the moment a file moves. The id includes pathless segments (`"/_site/login/"`), and the plugin rewrites it when a file moves
- The `.tanstackRouter` Vite plugin auto-generates `src/routeTree.gen.ts` — **do not edit it**
- Router context is typed via `RouterAppContext` in `__root.tsx`; thread data through context if needed across routes

**Paraglide i18n**

- Messages are defined in `project.inlang` (JSON-like format)
- Generates TypeScript in `src/paraglide/` — auto-included in the build
- Locale is **not** in the URL path (strategy: `["cookie", "preferredLanguage", "baseLocale"]`)
- Subscribe to locale changes with `subscribeToLocale()` / `getLocale()` (see `__root.tsx`)
- When locale changes, the root remounts (`key={locale}`) so all message calls use the new strings

**Styling**

- Tailwind CSS via Vite plugin + shadcn/ui from `@tessera/ui`
- Use `@tessera/ui/components/*` for buttons, forms, dialogs, toasts, etc.
- Theme provider wraps the app; toggle with next-themes
- Both light and dark modes are built in

## API Integration

### Upload Flow (INCOMPLETE)

The `upload()` function in `src/lib/upload.ts` orchestrates file/snippet/link uploads, but the three implementations are stubbed:

- `uploadFile()` — form-based file upload
- `uploadPaste()` — JSON POST for code/text snippets
- `uploadLink()` — JSON POST for URL redirects

Each should:

1. Make a fetch/XHR request to the Tessera backend (endpoint TBD from OpenAPI schema)
2. Track progress if `onProgress` is provided (XHR progress events)
3. Handle `signal` for cancellation (AbortSignal)
4. Return `{ fileName, shortUrl }` on success
5. Throw on network/validation errors

See `openapi.json` in the repo root for the exact endpoint signatures. The backend is co-located at `/` (same host/port in production).

### Making API Calls

- No generated client exists yet — raw `fetch()` is the pattern
- Base URL is inferred from window.location (no explicit config needed)
- CORS is not a concern (SPA is embedded in the same binary)
- Attach `credentials: "include"` for session-based auth endpoints

Example:

```typescript
const response = await fetch("/api/upload", {
  method: "POST",
  body: formData,
  signal: abortController.signal,
});
if (!response.ok) throw new Error(`Upload failed: ${response.status}`);
return response.json();
```

## State & Forms

- **Form validation**: React Hook Form + Zod (examples in login form)
- **State management**: Zustand for cross-route state if needed; prefer local component state for UI-only state
- **URL state**: TanStack Router search params for shareable state (e.g., sorting, filters)

## Locale & i18n Patterns

**Using messages in components:**

```typescript
import * as m from "@/paraglide/messages";

export function MyComponent() {
  return <button>{m.button_upload()}</button>;
}
```

**Runtime locale switching:**

- Call the locale subscription functions from `src/lib/locale.ts`
- Root component remounts on locale change, so no manual update logic is needed
- Form values survive the remount because they are tied to React Hook Form, not component state

## Authentication & Two-Mode Flow

The app operates in two modes on the same interface:

- **Anonymous uploads** → No auth required; endpoints should accept unauthenticated requests
- **Authenticated uploads** → User logs in; backend tracks upload history, quotas, or other features
- **Login/Sign-up** → Form posts to backend auth endpoint; stores session (likely cookie-based given embedded deployment)

Currently, the home page doesn't visibly differentiate between logged-in and anonymous users. This may need UI refinement (e.g., showing upload history for logged-in users, higher limits, etc.).

## Notes for Future Work

1. **Upload implementations** are stubbed in `src/lib/upload.ts` and need completion — each function must wire to backend endpoints (see OpenAPI schema)
2. **Share link retrieval** (`/s/$code/`) is currently a placeholder — needs to fetch content from backend and display it
3. **Login flow** (`/login/`) has the form UI but needs backend auth endpoint integration:
   - Email/password authentication
   - Google OAuth flow (client ID, redirect handling)
   - Session management (cookie, token storage, or context threading)
4. **Auth state management** — decide if user state should be:
   - Stored in Zustand (cross-route persistence)
   - Threaded through router context
   - Or derived from server state on each page load
5. **Monorepo dependencies**: This app depends on shared packages (`@tessera/ui`, `@tessera/i18n`); changes to those require a rebuild here

## Admin Panel

The operator console, at `/admin/*`. Its code is kept apart from the site's so the boundary stays visible: pages in `routes/admin/`, frame in `components/admin/`, query hooks in `hooks/admin/`, helpers in `lib/admin/`. Admin-only translation keys are in the same `messages/*.json` as the site's; prefix a new one with the page it belongs to (`users_…`, `settings_…`) or `admin_` when it would otherwise collide with a site key.

```
routes/admin/
├── route.tsx               # Layout: requireAuth, AdminShell around <Outlet />, header title from staticData
├── index.tsx               # No UI — redirects to the first page the account may open
├── users/                  # Accounts, roles, suspend / delete / end their shares (users:read)
├── all-shares/             # Every share on the instance, delete any (shares:read:any)
├── invites/                # Invite codes (invites:read)
├── dashboard/              # Stats, trends, storage locations, current settings, recent shares (stats:read)
├── roles/                  # Roles and what they permit (roles:read)
└── settings/               # route.tsx = grouped nav layout + sets the title, gates settings:read once; one route per section; index redirects to site
```

**Frame and titles**

`AdminShell` (sidebar + header + content padding) is rendered once by `routes/admin/route.tsx`, so moving between admin pages re-renders only the content, never the sidebar. A page does not render `AdminShell`; it sets `staticData: { title: m.some_title }` — the message **function**, not its result, because `staticData` is evaluated once at module load and a string would not follow a locale switch. The header shows the deepest match's title, so settings sub-pages inherit 系統設定 from `settings/route.tsx`. The type is declared on `StaticDataRouteOption` in `main.tsx`.

**Auth and permissions**

The panel has no login page of its own — it is the same SPA, so `requireAuth` sends an unauthenticated visitor to `/login` with `redirect` set, and the root `requirePasswordChanged` guard covers forced password changes for admin pages too.

**Motion.** `routes/admin/route.tsx` wraps the panel in `MotionConfig reducedMotion="user"`; `AdminShell` plays `PageTransition` (enter-only fade, no translate — transforms add scrollable overflow and flash a scrollbar; keyed by the first path segment after `/admin`) on the content area, and `settings/route.tsx` keys its own by full pathname so switching settings tabs only replays the right-hand side. Variants and the easing curve live in `lib/admin/motion.ts`; use `Stagger`/`StaggerItem` for card rows. Dashboard charts take `CHART_ANIMATION` and turn it off under `useReducedMotion()`; stat numbers use `@number-flow/react`. Enter-only on purpose — exit animations would hold the new page back, and TanStack's `Outlet` can't be frozen cheaply for `AnimatePresence`.

Authorization reads `permissions` off `/auth/session`. **Never branch on `is_admin`** — the field still exists for older clients but only reports "holds the admin or owner role", not what the account may actually do. `src/lib/admin/permissions.ts` holds the permission strings (mirroring `crates/tessera-core/src/permission.rs`, which is a fixed enum, not a table). `ADMIN_ENTRY_PERMISSIONS` in `@tessera/auth/permissions` decides whether the site Header shows the admin link at all; `LANDING_ORDER` must stay a subset of it.

`routes/admin/route.tsx` requires sign-in for the whole subtree (`requireAuth`, `src/lib/route-guard.ts`); each page then gates its own permission in `beforeLoad` via `requirePermission` (`src/lib/admin/route-guard.ts`), except the settings pages, which share one check in `settings/route.tsx`. Lacking a permission redirects to the first page that account _can_ open rather than showing a 403 — the order lives in `LANDING_ORDER`. An account that can open none is sent back to `/`. The sidebar filters itself by the same permissions, but that is cosmetic: the route guard and the server are what actually enforce anything.

Roles are database rows and one account holds at most one. An empty `roles` array ("未指派") and the seeded `guest` role are both zero permissions, but they mean different things — nobody has decided yet, versus somebody deliberately chose the most restricted role — so the UI must keep them visually distinct.

Admin query keys start with `"admin"`; `main.tsx` gives that prefix a 30-second `staleTime` so moving between admin pages does not refetch everything.

**Settings forms ↔ backend config mapping**

`src/lib/admin/settings-schema.ts` is the source of truth for every settings field. Each schema's comments map its fields back to the dotted keys in **`apps/server/SETTINGS.md`** — that document, not `config.rs`, is what to read first: it lists every key, its default, and what breaks when it is set wrong. Keys are snake_case on the wire and camelCase in the forms.

Three kinds of key are deliberately **not** editable fields, and `lib/admin/settings-schema.ts` says so per section:

- `FIXED_AT_BOOT` (e.g. `server.max_body_bytes`) — `PUT` refuses it. Because the endpoint validates a batch, including one would reject the legitimate keys sent alongside it. Show it with `ReadOnlySetting` instead.
- `SECRET` (e.g. `scan.authorization`) — never readable, never writable; environment only. `GET` returns `null` for its value, so `ReadOnlySetting` takes `value={null}` and only explains.
- `SEALED` (`auth.oidc.client_secret`, `auth.github.client_secret`, `auth.discord.client_secret`, `mail.username`, `mail.password`) — writable, never readable. `GET` returns a **boolean** saying whether one is set. Use `SealedField`: the input always starts empty, **blank means leave unchanged**, a value overwrites. Never load-then-edit one, or the first save wipes the stored secret.

Settings are one flat list of dotted keys, not a typed object: `GET /admin/settings` returns every key with its value, where that value came from, and whether this endpoint will change it. `hooks/admin/use-settings.ts` wraps that; `readSetting(data, key)` returns `unknown` because the shape depends on which key you asked for. Writes go through `useUpdateSettings` as `{key: value}` and are validated as a **set** — one bad key rejects the whole batch, so send only the fields that actually changed (see how `routes/admin/settings/-components/account-settings-form.tsx` uses `dirtyFields`), and put keys in the same request only when they need to take effect together.

**Every settings section is wired to that API.** They all go through `useSettingsForm` (`src/hooks/admin/use-settings-form.ts`), which owns the shared half: react-hook-form, the `settings:write` check, and a submit that sends only `dirtyFields` through `collectChanges`. Per-field `encoders` handle the two transforms that exist — `scopes` (form string ↔ server `string[]`) and SEALED fields (blank returns `undefined`, which drops the key from the batch).

Initial values are read once at mount, via `SettingsLoader`, so a background refetch cannot overwrite half-typed input.

**Settings page building blocks** (all in `routes/admin/settings/-components/`). Use these instead of hand-rolling layout, so every section reads the same:

- `settings/route.tsx` is the guard for the subtree, sets the 系統設定 title, and renders the grouped settings nav beside the `<Outlet />`. The left nav groups live in `settingsNavGroups()` (`settings-nav.tsx`) — grouped by what an operator wants to change, not by key prefix (`share.default_expiry_hours` sits on the uploads page). Under `lg` the nav collapses into a select. The main sidebar has a single "系統設定" link.
- `SettingsPage` = page title + one-line description; `SettingsCard` = one section with a title, an optional one-line description and an optional header `action`. A page is a stack of cards; one form may span several cards.
- Fields: `SettingRow` (label + description left, narrow control right, stacks on narrow cards — needs a `FieldGroup` parent for the container query), `NumberField` (takes a `unit`: bytes/kib/seconds/minutes/hours/days/times; shows the unit inside the input and a readable conversion such as "= 7 天"), `NumberInput` (just the input, for two-per-row like throttle counters), `SwitchField`, `SelectField`, `TextField`/`SealedField` (full width — URLs need the room), `SettingsNotice` (info/warning/danger callouts; don't hand-roll amber boxes).
- `SettingsFormFooter form={form}` is a sticky save bar: status on the left, 捨棄/儲存 on the right, and a `useBlocker` confirm when leaving with unsaved changes. Pass `surface="card"` when the form lives inside a card.

**Copy rules for settings** (#352): one or two sentences — what it does, the default, what raising/lowering it does. No code nouns (row, ticket, session, dotted keys) in the UI; the reasoning lives in `apps/server/SETTINGS.md`. Units go in the input, not the label.

The sign-in providers page is three cards, each its own form and its own request — batching them would let a bad OIDC issuer reject an unrelated GitHub change. Two constraints are mirrored client-side so a batch is not rejected server-side: `mail.host`/`mail.from`/`mail.public_url` must be all-set or all-empty, and `share.access_log.retention_days` must be at least 1 while the log is on.

Several of these keys break an instance quietly rather than loudly, so the forms carry the consequence in the UI, not just the field name. The ones that matter: `auth.cookie_secure: always` on anything less than full HTTPS makes login fail **silently** (the browser drops the cookie; no error anywhere); `server.trusted_proxies` set too broadly lets callers pick their own source address, which is what every throttle counts on; a zeroed `auth.throttle.*` pair locks out everyone (`source`) or silently disables the limit; `auth.throttle.lookup` is the only thing making short retrieval codes safe; and **`features.admin` off removes this panel's own API** — a stored setting beats the environment, so only `tessera-server settings reset features.admin` on the server plus a restart brings it back. Turning it (or `features.accounts`) off asks for a typed confirmation that shows that command.

`GuestAnonymousWarning` reads live server values rather than form state on purpose: it describes what this instance is actually configured to do, so it stays true regardless of which forms are wired. The page that owns `auth.default_role` passes its unsaved value in to preview the warning before saving.

**Notes for future work**

1. **Trends come from `GET /admin/stats/history`** (#410/#411): one entry per UTC day, recorded by the hourly sweep from the day that version was deployed, and a day the server was down has no entry. `routes/admin/dashboard/-components/trend-series.ts` fills those gaps with `null` so the chart breaks instead of drawing a straight line through them, and starts at the first recorded day rather than padding the requested range. Each metric (storage / shares / traffic) is its own chart with one y-axis — never put bytes and counts on two axes. Charts use `--primary` for the series that matters and `--muted-foreground` for context: the design system's `--chart-*` tokens are greys and cannot serve as categorical colours
2. **Dropped for having no backend key**: the 網站資訊 section (site title/description/SEO keywords, admin-entry-on-home) and `upload.allowedFileTypes`. Do not re-add them as editable fields until `settings::keys()` actually lists them — `site.title`/`site.description` are tracked in #278; the other three are a deliberate no, with reasons, in #279.
3. **Account state comes from `AdminUserResponse.state`** (`active` / `disabled` / `deleted`). The users page offers 停用 or 重新啟用 depending on it and badges suspended accounts; `isDeletedAccount` reads it too. Only `owner_username` on the all-shares page, which carries no state, still falls back to the `gone-` prefix (`isDeletedUsername` in `src/lib/admin/accounts.ts`). Never send `is_admin` on `PATCH /admin/users/{id}` unless changing it — it rewrites the role (`true` → owner, `false` → none)
4. **Storage page** — `storage.split_across_backends` and `storage.reconcile_*` are wired (#366); turning splitting on shows a warning that split files have no direct download. Keys with no form yet are tracked in their own issues: `speed.*` (#306), `scan.*` (#297), `features.encryption` (#292), OIDC `link_by_email` (#317) and per-provider `allow_registration` (#325). Where files actually live is not a `settings::keys()` key at all any more — each backend is a row in `storage_backends`, managed through `/admin/storage/backends` (`StorageBackendsCard` in `storage.tsx`), not through `PUT /admin/settings`. `storage.url`, `storage.capacity_bytes` (now per-backend `capacity_bytes`), and `storage.options` (now per-backend `options`) no longer exist as settings keys.
5. **No section uses tabs.** Each is its own route under the grouped settings nav; a long page is split into `SettingsCard`s instead. The throttle page shows all nine `auth.throttle.*` counters, with `source` first and labelled as the per-IP overall limit (#387) — it used to sit on the uploads page and read like an upload-only limit.
6. **Per-account quota** (#373) is the users page's 設定配額 dialog (`PATCH /admin/users/{id}` with `quota_bytes` / `max_file_bytes`; `null` = use the role's, `0` = unlimited). `AdminUserResponse` doesn't return `max_file_bytes`, so that field starts at 「不變更」 and is only sent when changed — tracked in #454.
7. **Managing a share after upload** (#364): `ManageShareDialog` (`routes/_site/-components/`) is shared by 我的分享 (session auth) and the home page's 最近上傳 list (anonymous, management token). Anonymous tokens are kept in `stores/recent-shares.ts` (localStorage) — that list is the keys to those shares, which is the trade-off for being able to come back to them.

## Performance & Accessibility

- Vite + Tailwind produce a lean production bundle; lazy-loading routes via router plugin is enabled
- Always include alt text on images, labels on inputs, semantic HTML, and keyboard support (enforced by Ultracite linting)
- Router uses scroll restoration by default

## Debugging

- TanStack Router DevTools are included (bottom-left corner in dev)
- Vite dev server with HMR enabled
- `pnpm check-types` catches routing and TypeScript errors before production build

---

# Ultracite Code Standards

This project uses **Ultracite**, a zero-config preset that enforces strict code quality standards through automated formatting and linting.

## Quick Reference

- **Format code**: `pnpm fix`
- **Check for issues**: `pnpm check`
- **Diagnose setup**: `pnpm dlx ultracite doctor`

Oxlint + Oxfmt (the underlying engine) provides robust linting and formatting. Most issues are automatically fixable.

---

## Core Principles

Write code that is **accessible, performant, type-safe, and maintainable**. Focus on clarity and explicit intent over brevity.

### Type Safety & Explicitness

- Use explicit types for function parameters and return values when they enhance clarity
- Prefer `unknown` over `any` when the type is genuinely unknown
- Use const assertions (`as const`) for immutable values and literal types
- Leverage TypeScript's type narrowing instead of type assertions
- Use meaningful variable names instead of magic numbers - extract constants with descriptive names

### Modern JavaScript/TypeScript

- Use arrow functions for callbacks and short functions
- Prefer `for...of` loops over `.forEach()` and indexed `for` loops
- Use optional chaining (`?.`) and nullish coalescing (`??`) for safer property access
- Prefer template literals over string concatenation
- Use destructuring for object and array assignments
- Use `const` by default, `let` only when reassignment is needed, never `var`

### Async & Promises

- Always `await` promises in async functions - don't forget to use the return value
- Use `async/await` syntax instead of promise chains for better readability
- Handle errors appropriately in async code with try-catch blocks
- Don't use async functions as Promise executors

### React & JSX

- Use function components over class components
- Call hooks at the top level only, never conditionally
- Specify all dependencies in hook dependency arrays correctly
- Use the `key` prop for elements in iterables (prefer unique IDs over array indices)
- Nest children between opening and closing tags instead of passing as props
- Don't define components inside other components
- Use semantic HTML and ARIA attributes for accessibility:
  - Provide meaningful alt text for images
  - Use proper heading hierarchy
  - Add labels for form inputs
  - Include keyboard event handlers alongside mouse events
  - Use semantic elements (`<button>`, `<nav>`, etc.) instead of divs with roles

### Error Handling & Debugging

- Remove `console.log`, `debugger`, and `alert` statements from production code
- Throw `Error` objects with descriptive messages, not strings or other values
- Use `try-catch` blocks meaningfully - don't catch errors just to rethrow them
- Prefer early returns over nested conditionals for error cases

### Code Organization

- Keep functions focused and under reasonable cognitive complexity limits
- Extract complex conditions into well-named boolean variables
- Use early returns to reduce nesting
- Prefer simple conditionals over nested ternary operators
- Group related code together and separate concerns

### State Management — Prop Drilling 的層數界線

用「這個值從擁有它的元件,到真正用它的元件,中間隔了幾層」來決定要不要改用 `zustand`。中間層只是把 prop 往下傳、自己根本沒用到,那一層就算數。

| 層數 | 做法 |
| --- | --- |
| 1 層 | 直接傳 prop,不要為了這個開 store |
| 2 層 | **要停下來評估**。同一組值會不會再往下傳?中間層是不是被迫接一堆用不到的 prop?會的話就換 store |
| 3 層以上 | **一定要用 `zustand`**,不要再往下傳 |

**為什麼**:中間層被迫宣告自己用不到的 prop,型別會跟著擴散,之後改一個欄位要動整條鏈上的每個檔案;而且中間任一層 re-render 都會連帶重畫下游。

**注意**:

- 層數是看**傳遞深度**,不是元件巢狀深度。父元件直接把值傳給孫元件(中間沒有轉手)只算一層
- 用 selector 訂閱(`useStore((s) => s.field)`)而不是整包拿,不然等於沒解決 re-render
- 只在單一元件內部用的狀態就留在 `useState`,不要什麼都塞進 store
- 伺服器資料歸 TanStack Query,不要複製一份進 zustand

### Security

- Add `rel="noopener"` when using `target="_blank"` on links
- Avoid `dangerouslySetInnerHTML` unless absolutely necessary
- Don't use `eval()` or assign directly to `document.cookie`
- Validate and sanitize user input

### Performance

- Avoid spread syntax in accumulators within loops
- Use top-level regex literals instead of creating them in loops
- Prefer specific imports over namespace imports
- Avoid barrel files (index files that re-export everything)
- Use proper image components (e.g., Next.js `<Image>`) over `<img>` tags

### Framework-Specific Guidance

**Next.js:**

- Use Next.js `<Image>` component for images
- Use `next/head` or App Router metadata API for head elements
- Use Server Components for async data fetching instead of async Client Components

**React 19+:**

- Use ref as a prop instead of `React.forwardRef`

**Solid/Svelte/Vue/Qwik:**

- Use `class` and `for` attributes (not `className` or `htmlFor`)

---

## Testing

- Write assertions inside `it()` or `test()` blocks
- Avoid done callbacks in async tests - use async/await instead
- Don't use `.only` or `.skip` in committed code
- Keep test suites reasonably flat - avoid excessive `describe` nesting

## React 元件寫法規範

核心口訣:**外 function,內 arrow**。目的是讓任何人打開檔案,第一眼就知道「這個檔案的主角是誰」。

### 1. 頂層宣告用 function declaration + 具名導出

頁面元件、共用 UI 元件、自訂 Hook 一律 `export function Name()`,不要 `const Name = () => {}`,不要 `export default`。

- 泛型語法乾淨:`function Table<T>(...)`,不必寫 `<T,>` 這種為了避開 JSX 解析的逗號。
- 有 hoisting:主角可以放檔案最上面,輔助元件放最下面。
- DevTools / stack trace 有名字,好除錯。
- 具名導出讓 rename、find-references、auto-import 都可靠,也避免同一個元件在不同檔案被叫成不同名字。

例外(仍用 default export):`vite.config.ts`、`tailwind.config.ts` 等工具設定檔。

```tsx
interface TableProps<T> {
  items: T[];
  renderRow: (item: T) => React.ReactNode;
}

// 主角置頂
export function Table<T>({ items, renderRow }: TableProps<T>) {
  return (
    <table>
      <tbody>
        {items.map((item) => (
          <TableRow key={item.id}>{renderRow(item)}</TableRow>
        ))}
      </tbody>
    </table>
  );
}

// 只有本檔用得到的小元件放最底部,靠 hoisting
function TableRow({ children }: { children: React.ReactNode }) {
  return <tr>{children}</tr>;
}
```

注意:hoisting 只對 `function` 成立。底部的輔助元件若寫成 `const`,執行時會炸 TDZ。

### 2. 元件內部邏輯用箭頭函式

大括號裡面的東西——event handler、`map`/`filter` 的 callback、`useEffect`/`useMemo`/`useCallback` 的參數——一律 arrow function。

```tsx
export function Counter() {
  const [count, setCount] = useState(0);

  const handleIncrement = () => setCount((prev) => prev + 1);

  useEffect(() => {
    console.log(`count: ${count}`);
  }, [count]);

  return (
    <div>
      <p>{count}</p>
      <button onClick={handleIncrement} type="button">
        ＋
      </button>
      <button onClick={() => setCount(0)} type="button">
        Reset
      </button>
    </div>
  );
}
```

命名慣例:本地 handler 用 `handleXxx`,往下傳的 props 用 `onXxx`。看到 `onSubmit={handleSubmit}` 就知道誰是定義方、誰是呼叫方。

### 3. 型別

- 不要用 `React.FC` / `React.FunctionComponent`。直接定義 `interface XxxProps`,在參數上解構標註。
- `React.FC` 不支援乾淨的泛型,而且會隱式帶進 `children`——要 children 就自己寫 `children: React.ReactNode`,型別才是誠實的。
- 型別 import 用 `import type { ... }`。

```tsx
// ❌
const Button: React.FC<ButtonProps> = ({ label }) => <button>{label}</button>;

// ✅
interface ButtonProps {
  label: string;
}

export function Button({ label }: ButtonProps) {
  return <button type="button">{label}</button>;
}
```

### 4. 檔案組織

- 一個檔案一個「主角」export。路由檔的主角是 `Route`,元件檔的主角是那個元件。
- 輔助元件放檔案底部;一旦第二個檔案要用它,就搬去 `-components/`(路由專屬)或 `components/`(跨路由共用)。
- 輔助元件變大或帶自己的狀態時就拆檔;純排版的長 JSX 不必為了行數硬拆。

TanStack Router 檔案已經是這個形狀,照抄即可——`Route` 物件置頂,`component` 指到下方的具名 function:

```tsx
export const Route = createFileRoute("/admin/dashboard/")({
  component: DashboardComponent,
  staticData: { title: m.dashboard_title },
});

function DashboardComponent() {
  return <StatsCards />;
}
```

### 5. 要包 HOC 時保留函式名字

`memo` / `forwardRef` 之類的包裝會讓元件在 DevTools 變成 `Anonymous`。包的時候把 function 寫成具名的:

```tsx
export const Row = memo(function Row({ item }: RowProps) {
  return <tr>{item.name}</tr>;
});
```

本專案優先「把 ref 當一般 prop 傳」,不用 `forwardRef`(見上面 React 段落)。

### 這些規則哪些是 lint 會擋的

根目錄 `vite.config.ts` 的 `lint` 針對 `apps/web/src/**` 開了三條:

- `import/no-default-export` — 擋 default export(app 根目錄的 `vite.config.ts` 等設定檔不在範圍內)。
- `react/function-component-definition` — 擋 `const Foo = () => <div />`。只看元件定義形式,元件內部的 handler、`map` callback 不受影響。
- `typescript/no-restricted-types` — 擋 `React.FC` / `React.FunctionComponent`。

`no-use-before-define` 全域關掉,所以「輔助元件放檔案底部」不會被 lint 擋。

lint 擋不到、要靠 code review 的部分:`handleXxx` / `onXxx` 命名、輔助元件何時該拆檔、底部 helper 誤寫成 `const`(TDZ)。(`react/jsx-handler-names` 有刻意關掉——它對 `field.onChange` 這種外部 API 的名字會系統性誤判,理由寫在根目錄 `vite.config.ts`。)

### 送出前自我檢查

1. 這是最外層的元件或 Hook 嗎 → `export function Name()`,不要 default export。
2. 這是 `onClick`、`map`、`useEffect` 裡的東西嗎 → arrow function。
3. 有沒有寫到 `React.FC` 或 `export default`?
4. 底部的輔助元件是 `function` 而不是 `const` 嗎?
5. handler 叫 `handleXxx`、props 叫 `onXxx` 嗎?

## When Oxlint + Oxfmt Can't Help

Oxlint + Oxfmt's linter will catch most issues automatically. Focus your attention on:

1. **Business logic correctness** - Oxlint + Oxfmt can't validate your algorithms
2. **Meaningful naming** - Use descriptive names for functions, variables, and types
3. **Architecture decisions** - Component structure, data flow, and API design
4. **Edge cases** - Handle boundary conditions and error states
5. **User experience** - Accessibility, performance, and usability considerations
6. **Documentation** - Add comments for complex logic, but prefer self-documenting code

---

Most formatting and common issues are automatically fixed by Oxlint + Oxfmt. Run `pnpm fix` (`vp check --fix`) before committing to ensure compliance.
