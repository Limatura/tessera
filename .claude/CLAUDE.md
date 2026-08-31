# CLAUDE.md

This file provides repo-wide guidance to Claude Code. App-specific standards live under each app's own `.claude/CLAUDE.md` and take precedence when working inside that app.

## Repo Layout

- `apps/web` — React frontend (anonymous upload + user auth flows). See [apps/web/.claude/CLAUDE.md](../apps/web/.claude/CLAUDE.md).
- `apps/admin` — React admin dashboard (operator console). See [apps/admin/.claude/CLAUDE.md](../apps/admin/.claude/CLAUDE.md).
- `apps/server` — Rust backend. All business logic lives here; `web` and `admin` are embedded into and served from this binary.
- `packages/` — shared workspace packages (`@tessera/ui`, `@tessera/i18n`, `@tessera/env`, `@tessera/config`, ...).

## Code Standards

- **apps/web, apps/admin**: Ultracite (Oxlint + Oxfmt) — full standards documented in each app's CLAUDE.md, not duplicated here.
- **apps/server**: Rust — see `apps/server/README.md` and standard `cargo fmt` / `cargo clippy`.

## Common Commands

```bash
pnpm dev:web              # Start apps/web dev server
pnpm dev:admin            # Start apps/admin dev server
pnpm check                # Format + lint across the monorepo
pnpm fix                  # Auto-fix linting and formatting issues
```
