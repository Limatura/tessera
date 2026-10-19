<p align="center">
  <img src="apps/web/public/favicon.svg" alt="" width="88" height="88" />
</p>

<h1 align="center">Tessera</h1>

<p align="center">Self-hosted file and note sharing that cleans up after itself.</p>

<p align="center">
  English · <a href="README.zh-TW.md">繁體中文</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0-blue" alt="License: AGPL-3.0" /></a>
</p>

https://github.com/user-attachments/assets/f1c95b78-8561-4184-9616-c57e3fa3bc5a

<p align="center">
  <img src="docs/images/screenshot-upload.png" alt="The upload page" width="49%" />
  <img src="docs/images/screenshot-share.png" alt="A shared snippet, as the recipient sees it" width="49%" />
</p>

Upload a file, a folder, a snippet of code or a link, and get back a short code and a URL. Whoever has them can open it until it expires or has been downloaded as many times as you allowed. Then it is deleted.

No account is needed to send something, unless you turn that off. Tessera runs as one binary with the web app built in, keeps its data in SQLite, PostgreSQL or MySQL, and stores files on local disk or in object storage.

## Who it is for

**Just you, and people you trust.** Run it on a NAS or a home server, behind your VPN, to move files between your own devices or hand them to friends. The defaults suit this: anonymous uploads on, SQLite and local disk, nothing else to set up. See the [personal example](tessera.personal.env.example).

**A public instance for strangers.** Anyone can upload, so every resource is spent by people you do not know. Cap the file size and the total storage, shorten the default lifetime, keep sign-up closed or invite-only, and point it at a malware scanner. See the [public example](tessera.public.env.example).

**A team or an organization.** Turn anonymous uploads off and let people sign in through your company's OpenID Connect provider. Roles control who may upload, who may see everyone's shares and who may change settings, and each account can have its own quota. PostgreSQL and S3-compatible storage scale past one machine.

## Features

### Under the hood

- **Big files can skip your server.** With S3 or Google Cloud Storage behind it, uploads go straight to the bucket and plain downloads are redirected there. A share with a password or a download limit always comes through the server, so those limits still hold.
- **Limits that hold under load.** A retrieval is counted in one conditional database update, so two people racing for a one-time share cannot both get it. Expiry is checked on every request, and code lookups are rate-limited.
- **Streams instead of buffering.** Memory use stays flat however large the file.
- **Storage you can mix.** Local disk, S3-compatible stores, Azure Blob, Google Cloud Storage and WebDAV, several at once, added or retired from the admin panel without a restart. One large file can even be split across buckets (off by default).
- **SQLite, PostgreSQL or MySQL**, each with its own hand-written migrations.
- **An API to build on.** The OpenAPI contract is generated from the server code, checked in CI and fuzz-tested. The CLI and API tokens use the same endpoints as the web app.
- **Quotas and limits** per role, per account and for anonymous uploads together, plus resumable uploads and optional malware scanning.

### For the people using it

- **Expiry and download limits.** Each share has a lifetime and, optionally, a maximum number of retrievals.
- **Passwords**, required before anything is shown or downloaded.
- **Previews.** Code is syntax-highlighted, and images, SVGs and Mermaid diagrams show in the page. Links pasted into chat apps get a preview card.
- **Folders and many files**, downloaded together as one archive.
- **Accounts when you want them.** Open, invite-only or closed sign-up; passwords, GitHub, Discord or any OpenID Connect provider.
- **Interface in English, Traditional Chinese and Japanese.**

## Admin panel

<p align="center">
  <img src="docs/images/screenshot-admin.png" alt="The admin dashboard" width="80%" />
</p>

Everything an operator needs is at `/admin`, and nearly every setting takes effect on the next request, without a restart:

- **Dashboard:** storage used and where it lives, traffic, trends, and warnings about risky settings.
- **Users:** suspend or delete accounts, change roles, set per-account quotas, and end everything one account uploaded.
- **Shares:** every share on the instance, including anonymous ones, with who uploaded it.
- **Invite codes, roles and permissions.**
- **Settings:** sign-in providers, upload limits, expiry defaults, storage backends, email, scanning, link previews and branding.

Without a browser, the same settings are available from the command line: `tessera-server settings list`, `settings set <key> <value>`.

## Quick start

With Docker, from a clone of this repository:

```bash
docker compose up -d
```

Then open <http://localhost:12923>. On first start the server creates an administrator account and prints its password in the log, once:

```bash
docker compose logs tessera | grep password
```

Before you put it on the internet, read the [public deployment example](tessera.public.env.example). Anonymous uploads are on by default, and without a storage limit anyone who can reach the server can fill its disk.

### From source

You need Rust and pnpm.

```bash
pnpm install
pnpm --filter web build
pnpm server:dev
```

## Configuration

Most settings live in the admin panel. The few that must be known before the server starts, such as the port, the database URL and the data directory, are environment variables:

```bash
TESSERA_SERVER__PORT=9000
TESSERA_DATABASE__URL=postgres://tessera:secret@localhost/tessera
TESSERA_SERVER__DATA_DIR=/var/lib/tessera
```

Two commented starting points: [a personal instance](tessera.personal.env.example) and [a public one](tessera.public.env.example). For systemd there is a [unit file](contrib/tessera.service).

### Storage

Files go to a local directory unless you add another backend in the admin panel.

| Backend | Notes |
| --- | --- |
| Local disk | The default |
| S3-compatible | Tested against MinIO, Garage, RustFS, SeaweedFS, VersityGW and s3proxy. AWS S3, Cloudflare R2 and Backblaze B2 are checked when credentials are available. |
| Azure Blob Storage | Tested against Azurite |
| Google Cloud Storage | Built in, not yet covered by tests |
| WebDAV | Small files only, up to `share.inline_threshold_bytes` (64 KiB by default) |

## Command-line client

Install `tess` for Linux or macOS. Windows builds are on the [releases page](https://github.com/Limatura/tessera/releases), and `cargo install --path apps/cli` builds it from source.

```bash
curl -fsSL https://raw.githubusercontent.com/Limatura/tessera/main/scripts/install.sh | sh
```

Until there is a stable release, add `-s -- --version <version>` with the release you want.

```bash
tess login https://share.example.com
tess send report.pdf --expires 24
tess get https://share.example.com/s/P8D72PSF
echo "hello" | tess send -
tess ls
tess rm P8D72PSF
tess
```

`tess` on its own opens a full-screen view to find files and send them without typing paths, and to manage your shares.

`--encrypt` seals the content before it leaves your machine; the key is the part of the link after `#`, which the server never sees.

No CLI at hand? `curl` will do, and prints the link:

```bash
curl -T report.pdf https://share.example.com/
echo "hello" | curl -T - https://share.example.com/
```

Options go in headers: `-H 'X-Share-Expires: 1d'`, `-H 'X-Share-Max-Downloads: 3'`, `-H 'X-Share-Password: …'`.

## Documentation

- [User and operator guide](https://github.com/Limatura/tessera-docs)
- [Server internals](apps/server/README.md), and the design notes in [docs/](docs/README.md)
- [API contract](openapi.json), generated from the server code

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Security problems go through [SECURITY.md](SECURITY.md), not the issue tracker.

## License

[AGPL-3.0](LICENSE). If you run a modified Tessera for other people, you must offer them its source.
