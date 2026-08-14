# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**@apps/web** is the React frontend for Tessera, an anonymous file courier service. The app supports two distinct user flows:

1. **Anonymous Mode** (home page `/`) — Users can upload files, code snippets, or links without authentication. They receive a retrieval code, and the payload self-destructs after expiration or reaching the retrieval limit. This is the primary use case.

2. **User Mode** (authenticated `/login/`) — Registered users can log in (email/password or Google OAuth) to access account features like upload history, saved shares, and higher limits. Sign-up is also available.

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

### Related Packages

- `@tessera/ui` — shared shadcn/ui component library
- `@tessera/i18n` — i18n utilities and message definitions
- `@tessera/env` — environment variable schemas

## Project Structure

```
src/
├── routes/                 # TanStack Router file-based routing
│   ├── __root.tsx         # Root layout (header, theme, locale)
│   ├── index.tsx          # Anonymous upload mode (no auth required)
│   ├── login/             # Auth flow (login + sign-up)
│   ├── s/$code/           # Share link retrieval (unauthenticated access)
│   └── -components/       # Private route-local components (prefix with -)
├── components/            # Shared app components
│   ├── header.tsx         # Nav with locale selector and theme toggle
│   ├── theme-provider.tsx
│   └── ...
├── lib/                   # Utilities
│   ├── upload.ts          # Upload orchestration (INCOMPLETE — see below)
│   ├── locale.ts          # Locale subscription + document application
│   └── ...
├── paraglide/             # Generated i18n files (auto-created by Paraglide)
└── index.css              # Tailwind entry point
```

**Convention:** Components local to a route use the `-components/` prefix (leading dash) so TanStack Router ignores them when scanning for route files.

### Route Roles

- **`/` (Anonymous Mode)** — The main interface for all users. Three upload tabs: File, Code (syntax highlighting), Link. No authentication gate.
- **`/login/`** — Login and sign-up form. Supports email/password and Google OAuth. Linked from the sign-up prompt on the home page.
- **`/s/$code/`** — Public share link retrieval. Any visitor can access this with a valid code; used to download or retrieve the shared content.

## Development

### Common Commands

```bash
# From monorepo root
pnpm dev:web              # Start dev server + watch (localhost:3001)
pnpm check                # Format + lint (Ultracite/Oxlint/Oxfmt)
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
  - `$param` → dynamic segment (`:param`)
  - Files starting with `-` are route-local helpers, not routes
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

## Performance & Accessibility

- Vite + Tailwind produce a lean production bundle; lazy-loading routes via router plugin is enabled
- Always include alt text on images, labels on inputs, semantic HTML, and keyboard support (enforced by Ultracite linting)
- Router uses scroll restoration by default

## Debugging

- TanStack Router DevTools are included (bottom-left corner in dev)
- Vite dev server with HMR enabled
- `pnpm check-types` catches routing and TypeScript errors before production build
