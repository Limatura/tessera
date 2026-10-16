# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Tessera serves a mixed audience across use cases: everyday internet users sharing sensitive or temporary content (passwords, personal documents, screenshots); developers and technical users exchanging code snippets, logs, and configuration without retention; and organizations with privacy-first deployment needs that self-host to guarantee data destruction and control over their infrastructure.

The primary job is ephemeral sharing — users need to hand out content with a time or retrieval limit, knowing it will automatically vanish.

## Product Purpose

Tessera is an anonymous file courier. Users upload a file, code snippet, or link, receive a retrieval code, and the payload self-destructs after expiration or reaching its retrieval limit. The service operates as a single self-hosted binary — no managed infrastructure, no third-party accounts, no tracking or analytics. Users who are authenticated can access features like upload history and higher quotas, but the anonymous mode is the primary flow.

Success means making ephemeral sharing so simple and fast that it becomes the default for sensitive temporary content, and so private that users trust it with information they would never send through email or a cloud drive.

## Positioning

Tessera's position rests on four pillars:

1. **Self-hosted ownership** — Users deploy and control the infrastructure (one binary). No intermediary, no data in external hands, no vendor lock-in.
2. **Ephemeral-first design** — Deletion is automatic and inevitable. Files expire or hit their retrieval limit; users cannot change their minds or decide to keep something "just in case." This is a feature, not a limitation.
3. **Frictionless simplicity** — Anonymous upload with no account required. Share in seconds. No signup flow blocking the primary task. Accounts are opt-in for advanced features (history, higher limits).
4. **Multi-platform flexibility** — The same backend API powers web, CLI, TUI, browser extension, and an admin console. A developer can script uploads; a user can share from their terminal; a team can manage uploads through a dashboard.

This combination is meaningfully different from file-sharing services that default to account-gated uploads, persistent storage, or cloud-only deployments.

## Operating Context

**Anonymous Mode (Primary Flow)**

Users land on the home page and immediately see three tabs: File, Code, Link. They can upload without logging in. Within seconds, they get a retrieval code and optionally a QR code. They share the code with someone else, who visits the share link and downloads/views the content. After the configured expiration time or retrieval count is reached, the payload is deleted.

**Authenticated Mode (Advanced Features)**

Users log in via email/password or Google OAuth. They gain access to upload history, saved shares, and higher upload limits. The upload flow remains the same; the difference is in persistence and quota management.

**Multi-Language Support**

Paraglide i18n is built in. Locale is negotiated via cookie, Accept-Language header, and baseLocale fallback — no URL path prefix. Users can switch languages in the header; the app remounts and messages update.

**Responsive Design**

The app must work seamlessly on desktop, tablet, and mobile. The anonymous upload interface is the same on all screen sizes; no degraded flow on smaller devices.

## Capabilities and Constraints

**Confirmed Capabilities**

- Anonymous file upload (progress tracking, cancellation support)
- Code snippet upload with syntax highlighting (multi-language CodeMirror)
- Link redirect upload
- QR code generation for share links
- Retrieval via public link with retrieval code or QR scan
- User authentication (email/password, Google OAuth)
- Upload history and analytics for authenticated users
- Configurable expiration and retrieval limits
- Multi-language UI with runtime locale switching
- Light and dark theme support

**Technical Constraints**

- Frontend is embedded in the Rust backend binary; no separate deployment.
- All business logic lives in the backend API. Frontend is purely presentation and orchestration.
- CORS is not a concern (SPA embedded in same binary).
- Session-based auth (cookie-based, credentials: "include" on API calls).

**Undecided Product Facts**

- Exact quota/limit tiers for authenticated users (feature scope open)
- Whether anonymous users can create accounts retroactively after uploading
- Admin console capabilities and audience (builder or operator?)
- TUI and extension feature parity (are all web features available there?)

## Brand Commitments

**Name & Positioning:** Tessera (derived from the tessellations pattern — small pieces fitting together). Brand position is self-hosted, privacy-conscious, and utilitarian.

**Aesthetic Intent:** Minimal and utilitarian as the foundation, with selective delight in key moments. The interface should feel professional and trustworthy. Avoid decoration that doesn't serve the job, but do invest in memorable interactions where they elevate the experience—like thoughtful animations on the upload flow that feel impressive without adding visual fatigue. Clarity and focus are paramount; flourish is the accent, not the baseline.

**Existing Assets:** README.md and OpenAPI schema document the architecture and API contract. Fumadocs site handles documentation.

## Evidence on Hand

- **Code:** Monorepo with React web app (Vite + TanStack Router), Rust backend, admin console, TUI, browser extension, docs site.
- **API:** OpenAPI schema (`openapi.json` at repo root) is the contract; generated by `utoipa` from backend handlers. CI enforces it stays in sync.
- **Deployment:** Docker image or source build; environment variables configure database (SQLite/Postgres/MySQL), storage backend (local/S3/GCS/Azure/HTTPS), and ports.
- **i18n:** Paraglide tooling with message definitions in `project.inlang`.
- **Styling:** Tailwind CSS + shadcn/ui components via shared `@tessera/ui` package.
- **Linting:** Ultracite (Oxlint + Oxfmt) enforces code quality, accessibility, and performance standards.

## Product Principles

1. **Ephemeral-first, not reluctant.** Automatic deletion is the point, not a limitation. Design for inevitability, not regret.
2. **Simple by design.** Fewer clicks, fewer decisions, fewer account gates. Anonymous mode is the default; advanced features do not complicate the core flow.
3. **Privacy is infrastructure.** Self-hosted ownership and no tracking are not marketing claims; they are architectural facts that shape every decision.
4. **Professional and utilitarian.** The interface is trustworthy and focused, avoiding decoration or complexity that does not serve the job.
5. **Accessible everywhere.** Multiple languages, multiple devices, and multiple input methods (web, CLI, TUI, extension, API) are equally supported.

## Accessibility & Inclusion

Ultracite linting (Oxlint) enforces WCAG standards, semantic HTML, keyboard navigation, and meaningful alt text as code quality baseline. Paraglide i18n supports multiple languages with runtime switching. The app must remain usable on all screen sizes (desktop, tablet, mobile) and by assistive technologies. These are non-negotiable product requirements, not optional enhancements.
