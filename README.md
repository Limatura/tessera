<p align="center">
  <img src="apps/web/public/favicon.svg" alt="" width="88" height="88" />
</p>

<h1 align="center">Tessera</h1>

<p align="center">Self-hosted file and note sharing that cleans up after itself.</p>

<p align="center">
  English · <a href="README.zh-TW.md">繁體中文</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/Limatura/tessera" alt="License: AGPL-3.0" /></a>
</p>

<p align="center">
  <img src="docs/images/screenshot-upload.png" alt="The upload page" width="49%" />
  <img src="docs/images/screenshot-share.png" alt="A shared snippet, as the recipient sees it" width="49%" />
</p>

Upload a file, a folder, a snippet of code or a link, and get back a short code and a URL. Whoever has them can open it until it expires or has been downloaded as many times as you allowed. Then it is deleted.

No account is needed to send something, unless you turn that off. Tessera runs as one binary with the web app built in, keeps its data in SQLite, PostgreSQL or MySQL, and stores files on local disk or in object storage.

## Who it is for

**Just you, and people you trust.** Run it on a NAS or a home server, behind
your VPN, to move files between your own devices or hand them to friends. The
defaults suit this: anonymous uploads on, SQLite and local disk, nothing else to
set up. See the [personal example](tessera.personal.env.example).

**A public instance for strangers.** Anyone can upload, so every resource is
spent by people you do not know. Cap the file size and the total storage,
shorten the default lifetime, keep sign-up closed or invite-only, and point it
at a malware scanner. See the [public example](tessera.public.env.example).

**A team or an organization.** Turn anonymous uploads off and let people sign in
through your company's OpenID Connect provider. Roles control who may upload,
who may see everyone's shares and who may change settings, and each account can
have its own quota. PostgreSQL and S3-compatible storage scale past one machine.

## Features

- **Expiry and download limits.** Each share has a lifetime and, optionally, a maximum number of retrievals. Expired content is removed on a schedule.
- **Passwords.** A share can require a password before anything is shown or downloaded.
- **Previews.** Code is syntax-highlighted, and images, SVGs and Mermaid diagrams show in the page. Links pasted into chat apps get a preview card.
- **Large files.** Uploads resume after a dropped connection, and several files download together as one archive.
- **Accounts, when you want them.** Sign-up can be open, invite-only or closed. Sign in with a password, GitHub, Discord or any OpenID Connect provider. Roles decide who may do what, and API tokens let scripts upload.
- **Optional malware scanning** through ClamAV or an HTTP scanner of your choice.
- **Interface in English, Traditional Chinese and Japanese.**

## Admin panel

<p align="center">
  <img src="docs/images/screenshot-admin.png" alt="The admin dashboard" width="80%" />
</p>

Everything an operator needs is at `/admin`, and nearly every setting takes
effect on the next request, without a restart:

- **Dashboard:** storage used and where it lives, traffic, trends, and warnings
  about risky settings.
- **Users:** suspend or delete accounts, change roles, set per-account quotas,
  and end everything one account uploaded.
- **Shares:** every share on the instance, including anonymous ones, with who
  uploaded it.
- **Invite codes, roles and permissions.**
- **Settings:** sign-in providers, upload limits, expiry defaults, storage
  backends, email, scanning, link previews and branding.

Without a browser, the same settings are available from the command line:
`tessera-server settings list`, `settings set <key> <value>`.

## Quick start

With Docker, from a clone of this repository:

```bash
docker compose up -d
```

Then open <http://localhost:8787>. On first start the server creates an administrator account and prints its password in the log, once:

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

```bash
cargo install --path apps/cli

tessera send report.pdf --server https://share.example.com --expires 24
tessera get https://share.example.com/s/P8D72PSF
echo "hello" | tessera send - --server https://share.example.com
```

`--encrypt` seals the content before it leaves your machine; the key is the part of the link after `#`, which the server never sees.

## Documentation

- [User and operator guide](https://github.com/Limatura/tessera-docs)
- [Server internals](apps/server/README.md), and the design notes in [docs/](docs/README.md)
- [API contract](openapi.json), generated from the server code

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Security problems go through [SECURITY.md](SECURITY.md), not the issue tracker.

## License

[AGPL-3.0](LICENSE). If you run a modified Tessera for other people, you must offer them its source.
