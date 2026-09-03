# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**@apps/admin** is the React admin dashboard for Tessera, an anonymous file courier service. It's an internal management console for operators — not exposed to end users. It covers server configuration (storage, uploads, sessions, OIDC), site branding, and monitoring (recent shares, storage trend).

Unlike `apps/web`, this app is **not** embedded in the Rust binary — `apps/server/build.rs` only mirrors `apps/web/dist`. Admin ships separately (its own `Dockerfile` + `nginx.conf`) and sits behind a reverse proxy that puts it on the same domain as the frontend, so the session cookie is shared. All business logic lives in the backend API; this frontend is purely presentation and orchestration.

## Tech Stack

- **React 19+** with TanStack Router v1
- **Vite** for bundling and dev server (port 3002; `apps/web` is 3001, so both can run at once)
- **TanStack Query** for server state (session, users, roles, invites)
- **TypeScript** for type safety
- **Tailwind CSS** + **shadcn/ui** (via @tessera/ui) for styling
- **Paraglide** for i18n (cookie + Accept-Language strategy, same as apps/web)
- **TanStack Table** for data tables (recent shares, etc.)
- **Recharts** for charts (storage trend)
- **dnd-kit** for drag-and-drop
- **React Hook Form** + **Zod** for settings forms
- **next-themes** for theme switching

### Related Packages

- `@tessera/ui` — shared shadcn/ui component library
- `@tessera/i18n` — i18n utilities and message definitions
- `@tessera/env` — environment variable schemas

## Project Structure

```
src/
├── routes/                      # TanStack Router file-based routing
│   ├── __root.tsx               # Root layout (theme, locale, toaster); context carries queryClient
│   ├── index.tsx                 # No UI — redirects to the first page the account may open
│   ├── shares.tsx                # The caller's own shares — the fallback landing page, auth only
│   ├── users.tsx                 # Accounts and their roles (users:read)
│   ├── invites.tsx               # Invite codes (invites:read)
│   ├── dashboard/                # Stats cards, storage trend, recent shares (stats:read)
│   ├── roles/                    # Roles and what they permit (roles:read)
│   ├── settings/                 # One route per section, all gated on settings:read
│   │   ├── index.tsx             # No UI — redirects /settings to /settings/site
│   │   ├── site.tsx / theme.tsx / account.tsx
│   │   ├── upload.tsx / storage.tsx / oidc.tsx
│   │   └── -components/
│   │       ├── settings-page.tsx           # Shell every section renders inside
│   │       ├── account-settings-form.tsx   # The one form wired to the real API
│   │       ├── guest-anonymous-warning.tsx # Shown on both account and upload
│   │       ├── *-settings-form.tsx         # site / theme / upload / storage / oidc
│   │       └── number-field.tsx            # Shared numeric input for the forms above
│   └── dnd-demo/
├── components/
│   ├── admin-shell.tsx           # Sidebar + header frame every page renders inside
│   ├── app-sidebar.tsx / nav-main.tsx / nav-user.tsx / site-header.tsx
│   └── theme-provider.tsx
├── hooks/
│   ├── use-auth.ts               # sessionQueryOptions / useSession / useLogin / useLogout
│   ├── use-admin.ts              # roles, permissions, users, invites, own shares + role mutations
│   └── use-settings.ts           # GET/PUT /admin/settings + readSetting(key)
├── lib/
│   ├── permissions.ts            # Permission constants, hasPermission, landing order, role labels
│   ├── route-guard.ts            # requireAuth / requirePermission for a route's beforeLoad
│   ├── web-url.ts                # Builds apps/web URLs from VITE_FRONTEND_URL (login lives there)
│   ├── settings-schema.ts        # Zod schemas for every settings form — source of truth for field shape
│   ├── format.ts
│   └── locale.ts
├── data/
│   └── dashboard-mock.json       # Placeholder data until dashboard APIs exist
├── paraglide/                    # Generated i18n files (auto-created by Paraglide)
└── index.css
```

## Development

### Common Commands

```bash
# From monorepo root
pnpm dev:admin            # Start dev server + watch
pnpm check                # Format + lint (Ultracite/Oxlint/Oxfmt)
pnpm fix                  # Auto-fix linting and formatting issues

# From apps/admin
pnpm dev                  # Start dev server (proxies /api to the backend on :8787)
pnpm build                # Vite production build
pnpm check-types          # Type check + build (catches routing issues)
```

### Key Concepts

**Auth and permissions**

There is no login page here. Admin and `apps/web` share a domain behind the reverse proxy, so they share the session cookie; an unauthenticated visitor is sent to the frontend's login page with a `redirect` back to where they were headed. `src/lib/web-url.ts` builds those URLs from `VITE_FRONTEND_URL` (empty = same origin at root; set it to `http://localhost:3001` in dev, where the two dev servers are on different ports).

Authorization reads `permissions` off `/auth/session`. **Never branch on `is_admin`** — the field still exists for older clients but only reports "holds the admin or owner role", not what the account may actually do. `src/lib/permissions.ts` holds the permission strings (mirroring `crates/tessera-core/src/permission.rs`, which is a fixed enum, not a table).

Every route gates itself in `beforeLoad` via `requireAuth` / `requirePermission` (`src/lib/route-guard.ts`). Lacking a permission redirects to the first page that account _can_ open rather than showing a 403 — the order lives in `LANDING_ORDER`, falling back to `/shares`, which needs only a session. The sidebar filters itself by the same permissions, but that is cosmetic: the route guard and the server are what actually enforce anything.

Roles are database rows and one account holds at most one. An empty `roles` array ("未指派") and the seeded `guest` role are both zero permissions, but they mean different things — nobody has decided yet, versus somebody deliberately chose the most restricted role — so the UI must keep them visually distinct.

**Settings forms ↔ backend config mapping**

`src/lib/settings-schema.ts` is the source of truth for every settings field. Each schema's comments map its fields back to `apps/server/src/config.rs` (`server.*`, `storage.*`, `share.*`, `auth.*`, `features.*`, snake_case → camelCase). Read this file before touching any settings form — it also flags which sections/fields are **not yet backed by the server** (marked "(新)" in the source: OIDC, site title/description/SEO, theme, admin-entry visibility, allowed upload types). Do not wire those to a real API call until the server exposes the corresponding config.

Settings are one flat list of dotted keys, not a typed object: `GET /admin/settings` returns every key with its value, where that value came from, and whether this endpoint will change it. `use-settings.ts` wraps that; `readSetting(data, key)` returns `unknown` because the shape depends on which key you asked for. Writes go through `useUpdateSettings` as `{key: value}` and are validated as a **set** — one bad key rejects the whole batch, so send only the fields that actually changed (see how `account-settings-form.tsx` uses `dirtyFields`), and put keys in the same request only when they need to take effect together.

**Only `account-settings-form.tsx` is wired to that API.** The other five sections are still local `useForm` state whose submit does nothing — their fields are either fixed at boot (`server.max_body_bytes`) or have no server config at all.

`GuestAnonymousWarning` reads live server values rather than form state on purpose: it describes what this instance is actually configured to do, so it stays true regardless of which forms are wired. The page that owns `auth.default_role` passes its unsaved value in to preview the warning before saving.

**TanStack Router (v1)**

- Same conventions as apps/web: file-based routes in `src/routes/`, `$param` for dynamic segments, `__root.tsx` for the root layout
- `.tanstackRouter` Vite plugin auto-generates `src/routeTree.gen.ts` — **do not edit it**

**Paraglide i18n**

- Same setup as apps/web: messages in `project.inlang`, generated into `src/paraglide/`, locale via cookie/Accept-Language (not URL-prefixed)
- Root component remounts on locale change (`key={locale}` in `__root.tsx`) — any unsaved form state is lost on locale switch; this is intentional (see comment in `__root.tsx`)

**Styling**

- Tailwind CSS via Vite plugin + shadcn/ui from `@tessera/ui`
- Dashboard layout uses `SidebarProvider`/`SidebarInset` from `@tessera/ui/components/sidebar`
- Default theme is `dark` (see `ThemeProvider` in `__root.tsx`)

## State & Forms

- **Form validation**: React Hook Form + Zod, schemas centralized in `src/lib/settings-schema.ts`
- **Data tables**: TanStack Table for `recent-shares-table.tsx`
- **Charts**: Recharts for `storage-trend-chart.tsx`
- **State management**: prefer local component state; escalate to Zustand only past 2+ levels of prop drilling (see Prop Drilling section below)

## Notes for Future Work

1. **Dashboard data is mocked** (`src/data/dashboard-mock.json`) — stats cards, storage trend, and recent shares need real backend endpoints
2. **Settings sections without a backend field yet**: OIDC, site title/description/SEO, theme, admin-entry-on-home visibility, allowed upload types — see the "(新)" markers in `settings-schema.ts`
3. **dnd-demo route** looks like a scratch/demo route — confirm with the team before building on it or removing it
4. **The users / invites / roles / shares pages are read-only lists.** The mutations they would need already exist in `use-admin.ts` (`useCreateRole`, `useUpdateRole`, `useDeleteRole`, `useSetUserRole`) but nothing calls them yet — no create/edit UI
5. **Five of the six settings sections still do nothing on submit** — only 帳號與註冊 (`account-settings-form.tsx`) talks to the server. 上傳設置 and 存儲配置 have real backing for most of their fields (`auth.throttle.source.*`, `storage.max_total_bytes`, `share.*_expiry_hours`, `storage.url`, `server.part_size_bytes`) and could be wired the same way; `server.max_body_bytes` cannot — it is fixed at boot
6. **No section uses tabs any more.** Each is its own route under the collapsible sidebar group. Add tabs back only when a single section grows enough sub-detail to need them (`auth.hashing.*` and the seven `auth.throttle.*` groups would be the first candidates, under 帳號與註冊)

## Debugging

- TanStack Router DevTools are included (bottom-left corner in dev)
- Vite dev server with HMR enabled
- `pnpm check-types` catches routing and TypeScript errors before production build

---

# Ultracite Code Standards

This project uses **Ultracite**, a zero-config preset that enforces strict code quality standards through automated formatting and linting.

## Quick Reference

- **Format code**: `pnpm dlx ultracite fix`
- **Check for issues**: `pnpm dlx ultracite check`
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
      <button onClick={handleIncrement} type="button">＋</button>
      <button onClick={() => setCount(0)} type="button">Reset</button>
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
export const Route = createRoute({
  component: DashboardComponent,
  getParentRoute: () => rootRoute,
  path: "/dashboard",
});

function DashboardComponent() {
  return <AdminShell title="儀表板">...</AdminShell>;
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

`oxlint.config.ts` 針對 `apps/web/src/**`、`apps/admin/src/**` 開了三條:

- `import/no-default-export` — 擋 default export(app 根目錄的 `vite.config.ts` 等設定檔不在範圍內)。
- `react/function-component-definition` — 擋 `const Foo = () => <div />`。只看元件定義形式,元件內部的 handler、`map` callback 不受影響。
- `typescript/no-restricted-types` — 擋 `React.FC` / `React.FunctionComponent`。

`no-use-before-define` 全域關掉,所以「輔助元件放檔案底部」不會被 lint 擋。

lint 擋不到、要靠 code review 的部分:`handleXxx` / `onXxx` 命名、輔助元件何時該拆檔、底部 helper 誤寫成 `const`(TDZ)。
(`react/jsx-handler-names` 有刻意關掉——它對 `field.onChange` 這種外部 API 的名字會系統性誤判,理由寫在 `oxlint.config.ts`。)

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

Most formatting and common issues are automatically fixed by Oxlint + Oxfmt. Run `pnpm dlx ultracite fix` before committing to ensure compliance.
