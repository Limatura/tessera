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
