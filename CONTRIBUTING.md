# Contributing

Thanks for wanting to help. Issues and pull requests are welcome in English or Chinese.

Found a security problem? Do not open an issue; see [SECURITY.md](SECURITY.md).

## Before you start

For anything bigger than a small fix, open an issue first and say what you plan to do. It saves you writing something we would ask you to do differently.

## Setting up

You need Rust (the exact version is pinned in `rust-toolchain.toml` and installed for you by rustup), a nightly toolchain for `rustfmt`, Node.js and pnpm.

```bash
pnpm install
pnpm --filter web build   # the server embeds the built web app
pnpm server:dev           # http://127.0.0.1:12923
pnpm dev:web              # the web app with hot reload, talking to that server
```

On first start the server creates an administrator and prints its password in the log, once.

## Layout

- `apps/server`: the Rust server. All business rules live here. Its [README](apps/server/README.md) covers configuration, storage, testing and the lint policy in detail.
- `apps/web`: the React app, the admin panel under `/admin` included.
- `apps/cli`, `apps/extension`: other clients of the same API.
- `crates/`, `packages/`: code shared between them.
- `docs/`: how the server behaves and why. Read the page for the area you are changing.

User documentation lives in its own repository, [tessera-docs](https://github.com/Limatura/tessera-docs).

## Checks

CI runs these, so running them first saves a round trip.

Server:

```bash
pnpm server:fmt                     # needs nightly rustfmt
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace
```

Web:

```bash
pnpm check        # lint and format
pnpm check-types
pnpm test
```

If you change an API handler, regenerate the contract and the client types and commit the results:

```bash
pnpm server:openapi
pnpm web:api:generate
cargo insta review   # accept the updated contract snapshot
```

## Pull requests

- Keep one change per pull request.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org): `fix: ...`, `feat(web): ...`. `pnpm commit` walks you through one.
- Add tests for behaviour you change. A bug fix should come with a test that fails without it.
- Changes to the web app are reviewed by the frontend maintainer before merging.

By contributing you agree that your work is licensed under the [AGPL-3.0](LICENSE), like the rest of the project.
