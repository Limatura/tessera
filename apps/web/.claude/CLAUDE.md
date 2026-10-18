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
│   └── admin/             # AdminShell, sidebar, nav, ⌘K menu, pager, list states — admin only
├── hooks/                 # Query hooks; admin ones under hooks/admin/
├── lib/                   # Utilities; admin ones under lib/admin/
│   ├── upload/            # Upload orchestration (see Upload Flow)
│   ├── locale.ts          # Locale subscription + document application
│   └── ...
├── paraglide/             # Generated i18n files (auto-created by Paraglide)
└── index.css              # Tailwind entry point
```

### PWA and the share target

`public/sw.js` is the origin's **only** service worker (a scope holds one): streamed downloads (`lib/download/save-stream.ts`), the Web Share Target, and the asset cache all live there. Register it only through `lib/pwa/service-worker.ts`; registering another script at `/` would replace it. The manifest is not a file here: the backend generates `/manifest.webmanifest` from `site.title` and the appearance (`apps/server/src/manifest.rs`). A share from the system share sheet posts to `/share-target`, the worker stores it in IndexedDB and opens `/?share={id}`, and the home page prefills the matching tab (`hooks/use-shared-draft.ts`). Android only; iOS has no share target.

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
- Locale is **not** in the URL path (strategy: `["cookie", "preferredLanguage", "custom-instance", "baseLocale"]`). `custom-instance` (defined in `lib/locale.ts`) reads `data-default-locale` from `<html>`, which the server writes from the admin's `site.language`; `baseLocale` (`zh-TW`) is only what the dev server falls back to
- Subscribe to locale changes with `subscribeToLocale()` / `getLocale()` (see `__root.tsx`)
- When locale changes, the root remounts (`key={locale}`) so all message calls use the new strings

**Styling**

- Tailwind CSS via Vite plugin + shadcn/ui from `@tessera/ui`
- Use `@tessera/ui/components/*` for buttons, forms, dialogs, toasts, etc.
- Theme provider wraps the app; toggle with next-themes
- Both light and dark modes are built in

## API Integration

### Upload Flow

The upload code lives in `packages/upload` (`@tessera/upload`), shared with `apps/extension`. It is framework-free: every operation returns a `Promise`, bound to one instance by `createUploader({ baseUrl, credentials, headers })`. This app's instance is `uploader` in `src/lib/uploader.ts` (its own origin, `credentials: "include"`); call `uploader.upload(...)`, `uploader.updateShare(...)`, `uploader.deleteShare(...)`, `uploader.toResult(...)`, and import types and `minutesUntil` from `@tessera/upload`.

- `upload.ts` — `upload(payload, { onProgress, signal, encryption })`, dispatching file / paste / link. A multi-file upload creates the share with the first file and attaches the rest one by one (in order, so progress is real); if any later file fails, the half-built share is deleted. A file larger than `max_upload_bytes` from `/capabilities` goes resumable instead of one `POST`.
- `resumable.ts` — `POST /uploads` sessions: parts go straight to the store when the session says `direct`, and fall back to `PATCH` for the rest of the upload the first time a direct `PUT` fails (a bucket without CORS looks exactly like that). `runResumableUpload` takes its I/O injected so `resumable.test.ts` can drive that fallback.
- `share-api.ts` — `createShare`, `updateShare`, `deleteShare`, `toResult`. Anonymous uploads authorize later changes with the `management_token` from the create response.
- `context.ts` — base URL, credentials, headers, and `send`: XHR where it exists (for upload progress), `fetch` in a service worker, which has no XHR.
- `languages/` (imported as `@tessera/upload/languages`) — `detectLanguage`, `LANGUAGE_LABELS`/`languageLabel`, `normalizeLanguage` (other tools' names and extensions) and `languageFromClassName` (a code block's `language-ts` / `highlight-source-js` class). The code tab, the share page, `lib/paste-source.ts` and the extension all read these; `languages/index.ts` lists every place a new language has to be added.

Retrieving a share (`/s/$code`) goes through `hooks/use-share.ts`: `useShare` is free to call, `useShareTicket` spends one retrieval, which is why it has no `queryOptions` to preload with.

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

1. **Login flow** (`/login/`) has the form UI but needs backend auth endpoint integration:
   - Email/password authentication
   - Google OAuth flow (client ID, redirect handling)
   - Session management (cookie, token storage, or context threading)
2. **Auth state management** — decide if user state should be:
   - Stored in Zustand (cross-route persistence)
   - Threaded through router context
   - Or derived from server state on each page load
3. **Monorepo dependencies**: This app depends on shared packages (`@tessera/ui`); changes to those require a rebuild here

## Admin Panel

The operator console, at `/admin/*`. Its code is kept apart from the site's so the boundary stays visible: pages in `routes/admin/`, frame in `components/admin/`, query hooks in `hooks/admin/`, helpers in `lib/admin/`. Admin-only translation keys are in the same `messages/*.json` as the site's; prefix a new one with the page it belongs to (`users_…`, `settings_…`) or `admin_` when it would otherwise collide with a site key.

```
routes/admin/
├── route.tsx               # Layout: requireAuth, AdminShell around <Outlet />, header breadcrumb from staticData
├── index.tsx               # No UI — redirects to the first page the account may open
├── users/                  # Accounts, roles, suspend / delete / end their shares (users:read)
├── all-shares/             # Every share on the instance, delete any (shares:read:any)
├── invites/                # Invite codes (invites:read)
├── dashboard/              # Stats, trends, storage locations, current settings, recent shares (stats:read)
├── roles/                  # Roles and what they permit (roles:read)
└── settings/               # route.tsx = grouped nav layout + sets the title, gates settings:read once; one route per section; index redirects to site
```

**Frame and titles**

`AdminShell` (sidebar + header + content padding) is rendered once by `routes/admin/route.tsx`, so moving between admin pages re-renders only the content, never the sidebar. A page does not render `AdminShell`; it sets `staticData: { title: m.some_title }` — the message **function**, not its result, because `staticData` is evaluated once at module load and a string would not follow a locale switch. The header is a breadcrumb of every matched route with a title (`settings/route.tsx` + each settings tab gives "System Settings › Email"); on phones it collapses to a back link plus the current title. The type is declared on `StaticDataRouteOption` in `main.tsx`.

The sidebar's pages and groups live in `useAdminNav()` (`components/admin/nav-items.tsx`), filtered by permission; the ⌘K menu (`components/admin/command-menu.tsx`) reads the same list plus `settingsNavGroups()` (`lib/admin/settings-nav.ts`), so a new page or settings tab added there shows up in both. The sidebar collapses to an icon rail on desktop; language and sign-out are in the account menu.

**Lists and dialogs on phones.** Every admin list renders a card list under `md` and the table from `md` up (both in the DOM, switched with `md:hidden` / `hidden md:block`, so the first paint is right). Loading, failed and empty states use `ListSkeleton` / `ListError` (with retry) / `ListEmpty` from `components/admin/list-state.tsx`. Dialogs use `ResponsiveDialog*` from `@tessera/ui/components/responsive-dialog` — a Dialog (or AlertDialog with `alert`) from `md` up, a bottom Drawer below — never `Dialog`/`AlertDialog` directly; its `className` sizes the desktop popup only.

**Auth and permissions**

The panel has no login page of its own — it is the same SPA, so `requireAuth` sends an unauthenticated visitor to `/login` with `redirect` set, and the root `requirePasswordChanged` guard covers forced password changes for admin pages too.

**Motion.** `routes/admin/route.tsx` wraps the panel in `MotionConfig reducedMotion="user"`; `AdminShell` plays `PageTransition` (enter-only fade, no translate — transforms add scrollable overflow and flash a scrollbar; keyed by the first path segment after `/admin`) on the content area, and `settings/route.tsx` keys its own by full pathname so switching settings tabs only replays the right-hand side. Variants and the easing curve live in `lib/admin/motion.ts`; use `Stagger`/`StaggerItem` for card rows. Dashboard charts take `CHART_ANIMATION` and turn it off under `useReducedMotion()`; stat numbers use `@number-flow/react`. Enter-only on purpose — exit animations would hold the new page back, and TanStack's `Outlet` can't be frozen cheaply for `AnimatePresence`.

Authorization reads `permissions` off `/auth/session`. **Never branch on `is_admin`** — the field still exists for older clients but only reports "holds the admin or owner role", not what the account may actually do. `src/lib/admin/permissions.ts` holds the permission strings (mirroring `crates/tessera-core/src/permission.rs`, which is a fixed enum, not a table). `ADMIN_ENTRY_PERMISSIONS` in `@tessera/auth/permissions` decides whether the site Header shows the admin link at all; `LANDING_ORDER` must stay a subset of it.

`routes/admin/route.tsx` requires sign-in for the whole subtree (`requireAuth`, `src/lib/route-guard.ts`); each page then gates its own permission in `beforeLoad` via `requirePermission` (`src/lib/admin/route-guard.ts`), except the settings pages, which share one check in `settings/route.tsx`. Lacking a permission redirects to the first page that account _can_ open rather than showing a 403 — the order lives in `LANDING_ORDER`. An account that can open none is sent back to `/`. The sidebar filters itself by the same permissions, but that is cosmetic: the route guard and the server are what actually enforce anything.

Roles are database rows and one account holds at most one. An empty `roles` array ("Unassigned") and the seeded `guest` role are both zero permissions, but they mean different things — nobody has decided yet, versus somebody deliberately chose the most restricted role — so the UI must keep them visually distinct.

Admin query hooks are one file per domain under `hooks/admin/` (`use-users.ts`, `use-roles.ts`, `use-all-shares.ts`, `use-invites.ts`, `use-stats.ts`, `use-storage-backends.ts`, `use-scanner.ts`, `use-settings.ts`). Every key comes from `adminKeys` in `hooks/admin/keys.ts` — never write an `["admin", …]` literal: the no-argument entries (`adminKeys.users()`) are prefixes for `invalidateQueries`, the ones with arguments (`adminKeys.usersPage(offset, deleted)`) are single cache entries for `queryOptions`. All of them start with `"admin"`, and `main.tsx` gives that prefix a 30-second `staleTime` so moving between admin pages does not refetch everything. A query that needs a permission the page itself does not gate on (e.g. `useRoles()` needs `roles:read`) belongs in the component that is only mounted when that permission is held, not at the top of the page.

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

- `settings/route.tsx` is the guard for the subtree, sets the System Settings title, and renders the grouped settings nav beside the `<Outlet />`. The left nav groups live in `settingsNavGroups()` (`lib/admin/settings-nav.ts`, shared with the ⌘K menu) — grouped by what an operator wants to change, not by key prefix (`share.default_expiry_hours` sits on the uploads page). Under `lg` the nav collapses into a select. The main sidebar has a single "Settings" link.
- `SettingsPage` = page title + one-line description; `SettingsCard` = one section with a title, an optional one-line description and an optional header `action`. A page is a stack of cards; one form may span several cards.
- Fields: `SettingRow` (label + description left, narrow control right, stacks on narrow cards — needs a `FieldGroup` parent for the container query), `NumberField` (takes a `unit` — the unit the server stores: bytes/kib/seconds/minutes/hours/days, or times; time and size units get a unit menu beside the number that starts on the largest whole unit (900 seconds → 15 minutes) while the form value stays in the stored unit — conversions in `lib/admin/units.ts`), `NumberInput` (just the input, for two-per-row like throttle counters), `SwitchField`, `SelectField`, `TextField`/`SealedField` (full width — URLs need the room), `SettingsNotice` (info/warning/danger callouts; don't hand-roll amber boxes).
- **Reset to defaults** (`settings/reset.tsx`, System group) lists every key with `source: "panel"`, grouped by the page that edits it (`settingsSectionOf` in `lib/admin/settings-sections.ts` — add a rule there when a new key gets a page), and clears a section or all of them with `{key: null}`. It previews first (`usePreviewChanges(clearing(keys))`) and sends `confirm: true` only after showing the consequences. `features.admin` / `features.accounts` are skipped when their fallback is `false`, since resetting them would lock the admin out. `GET /admin/settings` returns each key's `fallback` (env or built-in default) for this. The full table of defaults lives in the docs (`apps/docs/content/docs/*/(core)/defaults.mdx`).
- `SettingsFormFooter form={form}` is a sticky save bar: status on the left, Discard/Save on the right, and a `useBlocker` confirm when leaving with unsaved changes. When a save is refused with `confirmation_required` (a shorter retention period would delete what is stored), `useSettingsForm` asks `POST /admin/settings/preview` for the numbers and holds the save; the footer shows them in a dialog (translated by the consequence's `kind`, `lib/admin/consequences.ts`), and "Save anyway" sends the same batch with `confirm: true`. Cancel keeps the edits unsaved. Pass `surface="card"` when the form lives inside a card.

**Copy rules for settings** (#352): one or two sentences — what it does, the default, what raising/lowering it does. No code nouns (row, ticket, session, dotted keys) in the UI; the reasoning lives in `apps/server/SETTINGS.md`. Units go in the input, not the label.

The sign-in providers page is three cards, each its own form and its own request — batching them would let a bad OIDC issuer reject an unrelated GitHub change. Two constraints are mirrored client-side so a batch is not rejected server-side: `mail.host`/`mail.from`/`mail.public_url` must be all-set or all-empty, and `share.access_log.retention_days` must be at least 1 while the log is on.

Several of these keys break an instance quietly rather than loudly, so the forms carry the consequence in the UI, not just the field name. The ones that matter: `auth.cookie_secure: always` on anything less than full HTTPS makes login fail **silently** (the browser drops the cookie; no error anywhere); `server.trusted_proxies` set too broadly lets callers pick their own source address, which is what every throttle counts on; a zeroed `auth.throttle.*` pair locks out everyone (`source`) or silently disables the limit; `auth.throttle.lookup` is the only thing making short retrieval codes safe; and **`features.admin` off removes this panel's own API** — a stored setting beats the environment, so only `tessera-server settings reset features.admin` on the server plus a restart brings it back. Turning it (or `features.accounts`) off asks for a typed confirmation that shows that command.

`GuestAnonymousWarning` reads live server values rather than form state on purpose: it describes what this instance is actually configured to do, so it stays true regardless of which forms are wired. The page that owns `auth.default_role` passes its unsaved value in to preview the warning before saving.

**Notes for future work**

1. **Trends come from `GET /admin/stats/history`** (#410/#411): one entry per UTC day, recorded by the hourly sweep from the day that version was deployed, and a day the server was down has no entry. `routes/admin/dashboard/-components/trend-series.ts` fills those gaps with `null` so the chart breaks instead of drawing a straight line through them, and starts at the first recorded day rather than padding the requested range. Each metric (storage / shares / traffic) is its own chart with one y-axis — never put bytes and counts on two axes. Charts use `--primary` for the series that matters and `--muted-foreground` for context: the design system's `--chart-*` tokens are greys and cannot serve as categorical colours
2. **Dropped for having no backend key**: the site information section (site title/description/SEO keywords, admin-entry-on-home) and `upload.allowedFileTypes`. Do not re-add them as editable fields until `settings::keys()` actually lists them — `site.title`/`site.description` are tracked in #278; the other three are a deliberate no, with reasons, in #279.
3. **Account state comes from `AdminUserResponse.state`** (`active` / `disabled` / `deleted`). The users page offers Disable or Re-enable depending on it and badges suspended accounts; `isDeletedAccount` reads it too. Only `owner_username` on the all-shares page, which carries no state, still falls back to the `gone-` prefix (`isDeletedUsername` in `src/lib/admin/accounts.ts`). Never send `is_admin` on `PATCH /admin/users/{id}` unless changing it — it rewrites the role (`true` → owner, `false` → none)
4. **Storage page** — `storage.split_across_backends` and `storage.reconcile_*` are wired (#366); turning splitting on shows a warning that split files have no direct download. Keys with no form yet are tracked in their own issues: `speed.*` (#306) and `scan.*` (#297); `features.encryption` (#292) is on the share page, not the features page: it is a rule about shares, not a site-wide switch. OIDC `link_by_email` (#317) and per-provider `allow_registration` (#325) are on the sign-in providers page; with per-provider registration, `ProviderRegistrationNote` sits in each card and combines that card's value with the global `auth.registration`. Where files actually live is not a `settings::keys()` key at all any more — each backend is a row in `storage_backends`, managed through `/admin/storage/backends` (`StorageBackendsCard` in `storage.tsx`), not through `PUT /admin/settings`. It is a card grid at every width (one column below `md`, two from `md` up), not the card-list/table pair. Each card is the gauge: its background fills from the left in the provider's colour as far as the backend is full, with a moving wave on the leading edge (`wave-edge.tsx`), amber/red past the `UsageBar` thresholds; a provider tile (`lib/admin/storage-provider.ts`) sits on the left, the used figure in large type on the right, and the default gets a sliding ring. The Add button is in the card header, so it stays reachable however many backends there are. `BackendResponse.url` is the location without credentials, `access_key` the masked key ID and `config` the allowlisted options that place the store (endpoint, region, allow_http, Azure account). The editor asks by type (S3-compatible, local, GCS, Azure, WebDAV, raw URL) and `lib/admin/backend-location.ts` turns the fields into `url` + `options` — S3/GCS/Azure keys go in options, never the URL, since `object_store` silently drops `key:secret@` from `s3://`. Blank secrets keep the stored ones unless anything else that places the store changed (`backend_credentials_required`, see `apps/server/src/storage/location.rs`). `storage.url`, `storage.capacity_bytes` (now per-backend `capacity_bytes`), and `storage.options` (now per-backend `options`) no longer exist as settings keys.
5. **No section uses tabs.** Each is its own route under the grouped settings nav; a long page is split into `SettingsCard`s instead. The throttle page shows all nine `auth.throttle.*` counters, with `source` first and labelled as the per-IP overall limit (#387) — it used to sit on the uploads page and read like an upload-only limit.
6. **Per-account quota** (#373) is the users page's Set quota dialog (`PATCH /admin/users/{id}` with `quota_bytes` / `max_file_bytes`; `null` = use the role's, `0` = unlimited). `AdminUserResponse` doesn't return `max_file_bytes`, so that field starts as "no change" and is only sent when changed — tracked in #454.
7. **Managing a share after upload** (#364): `ManageShareDialog` (`routes/_site/-components/`) is shared by My shares (session auth) and the home page's Recent uploads list (anonymous, management token). Anonymous tokens are kept in `stores/recent-shares.ts` (localStorage) — that list is the keys to those shares, which is the trade-off for being able to come back to them.

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

### State Management — how many levels of prop drilling

Decide whether to switch to `zustand` by how many levels a value passes through between the component that owns it and the one that uses it. A level counts when it only hands the prop down and does not use it itself.

| Levels | What to do |
| --- | --- |
| 1 | Pass the prop. Do not open a store for this |
| 2 | **Stop and weigh it.** Will the same values go further down? Is the middle level forced to take props it does not use? If so, switch to a store |
| 3 or more | **Use `zustand`.** Do not pass it further |

**Why**: a middle level forced to declare props it does not use spreads the types, so changing one field later touches every file along the chain, and a re-render anywhere in the middle redraws everything below it.

**Note**:

- Count **how far the value is passed**, not how deep the components are nested. A parent passing a value straight to a grandchild (nobody in between handing it on) is one level
- Subscribe with a selector (`useStore((s) => s.field)`), not the whole store, or the re-renders are not solved
- State used inside a single component stays in `useState`; do not put everything in a store
- Server data belongs to TanStack Query; do not copy it into zustand

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

## Writing React components

The rule in one line: **function outside, arrow inside**. Anyone opening a file should see at a glance what the file is about.

### 1. Top-level declarations are function declarations with named exports

Page components, shared UI components and custom hooks are always `export function Name()`: not `const Name = () => {}`, not `export default`.

- Clean generics: `function Table<T>(...)`, no `<T,>` comma to dodge the JSX parser.
- Hoisting: the main export can go at the top of the file and helpers at the bottom.
- DevTools and stack traces show a name, which helps debugging.
- Named exports make rename, find-references and auto-import reliable, and stop one component being called different names in different files.

Exception (still a default export): tool config files such as `vite.config.ts` and `tailwind.config.ts`.

```tsx
interface TableProps<T> {
  items: T[];
  renderRow: (item: T) => React.ReactNode;
}

// The main export at the top
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

// Small components only this file uses go at the bottom, relying on hoisting
function TableRow({ children }: { children: React.ReactNode }) {
  return <tr>{children}</tr>;
}
```

Note: hoisting only works for `function`. A helper at the bottom written as `const` throws a TDZ error at run time.

### 2. Logic inside a component uses arrow functions

Everything inside the braces (event handlers, `map`/`filter` callbacks, the arguments to `useEffect`/`useMemo`/`useCallback`) is an arrow function.

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

Naming: local handlers are `handleXxx`, props passed down are `onXxx`. `onSubmit={handleSubmit}` then shows which side defines it and which calls it.

### 3. Types

- No `React.FC` or `React.FunctionComponent`. Define `interface XxxProps` and annotate the destructured parameter.
- `React.FC` has no clean generics and adds `children` implicitly. If a component takes children, write `children: React.ReactNode` yourself, so the type tells the truth.
- Type imports use `import type { ... }`.

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

### 4. File layout

- One main export per file. In a route file it is `Route`; in a component file, that component.
- Helpers go at the bottom of the file. Once a second file needs one, move it to `-components/` (route-specific) or `components/` (shared across routes).
- Split a helper into its own file when it grows or holds its own state; long JSX that is only layout does not need splitting to save lines.

TanStack Router files already have this shape; copy it. The `Route` object at the top, its `component` pointing at a named function below:

```tsx
export const Route = createFileRoute("/admin/dashboard/")({
  component: DashboardComponent,
  staticData: { title: m.dashboard_title },
});

function DashboardComponent() {
  return <StatsCards />;
}
```

### 5. Keep the function's name when wrapping it in a HOC

Wrappers such as `memo` and `forwardRef` make a component show up as `Anonymous` in DevTools. Write the wrapped function with a name:

```tsx
export const Row = memo(function Row({ item }: RowProps) {
  return <tr>{item.name}</tr>;
});
```

This project passes ref as an ordinary prop rather than using `forwardRef` (see the React section above).

### Which of these rules lint enforces

The `lint` section of the root `vite.config.ts` turns on three rules for `apps/web/src/**`:

- `import/no-default-export`: refuses default exports (config files such as each app's root `vite.config.ts` are out of scope).
- `react/function-component-definition`: refuses `const Foo = () => <div />`. It only looks at how components are defined; handlers and `map` callbacks inside a component are unaffected.
- `typescript/no-restricted-types`: refuses `React.FC` and `React.FunctionComponent`.

`no-use-before-define` is off everywhere, so helpers at the bottom of a file are not flagged.

What lint cannot catch and review has to: `handleXxx`/`onXxx` naming, when a helper should get its own file, and a bottom helper mistakenly written as `const` (TDZ). (`react/jsx-handler-names` is deliberately off: it misfires on names that belong to another API, such as `field.onChange`; the reason is in the root `vite.config.ts`.)

### Check before sending

1. Is this a top-level component or hook? → `export function Name()`, no default export.
2. Is this inside `onClick`, `map` or `useEffect`? → arrow function.
3. Any `React.FC` or `export default`?
4. Are the helpers at the bottom `function`, not `const`?
5. Are handlers `handleXxx` and props `onXxx`?

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
