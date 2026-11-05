# Tessera on Railway

[![Deploy on Railway](https://railway.com/button.svg)](https://railway.com/deploy/tessera)

# Deploy and Host

Deploy Tessera from the published container image `ghcr.io/limatura/tessera:latest` and host a private file-sharing service on Railway. The template includes the service variables and a persistent volume mounted at `/data`.

## About Hosting

Railway runs Tessera as a single image-based service. The included 500 MB volume stores the SQLite database, encryption key, logs, and local uploads. Increase the volume size or configure an external storage backend as your instance grows.

## Why Deploy

Tessera gives you privacy-preserving, self-hosted sharing without relying on a third-party file transfer service. Railway provides managed deployments, public networking, logs, and persistent storage while the application remains open source and portable.

## Common Use Cases

- Share files, folders, notes, code snippets, and URLs with expiring links.
- Run a private transfer service for a small team or household.
- Host a controlled public upload endpoint with quotas and upload limits.
- Provide an API-backed file sharing service for internal tools and automation.

## Dependencies for Tessera

### Deployment Dependencies

- A Railway service using `ghcr.io/limatura/tessera:latest`.
- A persistent Railway volume mounted at `/data`.
- `RAILWAY_RUN_UID=0` so Railway can initialize the volume for the image's non-root runtime user.
- `TESSERA_SERVER__PORT=12923`, matching the image's exposed HTTP port.

Tessera is a self-hosted file and note sharing service that creates short-lived links for files, folders, code snippets, and URLs.

It is designed for privacy-conscious individuals, teams, and public sharing instances. Shares can expire automatically, require a password, or be limited by download count.

## Features

- Self-hosted file, folder, note, code, and link sharing
- Expiring share links
- Optional passwords and download limits
- Anonymous uploads or authenticated accounts
- Image, code, SVG, and Mermaid previews
- SQLite support out of the box
- PostgreSQL and MySQL support for larger deployments
- Persistent storage through Railway volumes
- Built-in administration panel at `/admin`
- API and command-line client support
- Optional object storage backends
- English, Traditional Chinese, and Japanese interfaces
- Single Rust binary with the web application embedded

## Deploy on Railway

1. Click the **Deploy on Railway** button above.
2. Select or create a Railway project.
3. Review the generated variables.
4. Add a persistent volume mounted at `/data`.
5. Deploy the service.
6. Open the generated Railway domain.

The service runs the published container image `ghcr.io/limatura/tessera:latest`. Tessera listens on port `12923` and stores its database, encryption key, and local files under `/data`.

The template does not build the application from source. Railway pulls the published GHCR image directly, so deployments are fast and reproducible.

## Railway Configuration

| Variable | Required | Default | Description |
| --- | --- | --- | --- |
| `PORT` | Automatically provided | Railway-managed | Internal port assigned by Railway. Do not set this manually. |
| `TESSERA_SERVER__HOST` | Yes | `0.0.0.0` | Binds Tessera to all container interfaces so Railway can route public traffic to it. |
| `TESSERA_SERVER__PORT` | Yes | `12923` | Internal HTTP port exposed by the Tessera image. Configure the Railway service domain to target port `12923`. |
| `TESSERA_DATABASE__URL` | Yes | `sqlite:///data/tessera.db?mode=rwc` | Stores the Tessera database on the persistent `/data` volume. |
| `TESSERA_SERVER__DATA_DIR` | Yes | `/data` | Stores the database, encryption key, and local file data. |
| `RAILWAY_RUN_UID` | Yes for the included volume | `0` | Allows the non-root image to initialize and write to the Railway volume mounted at `/data`. |
| `TESSERA_TELEMETRY__FORMAT` | No | `json` | Emits structured logs that are easier to search in Railway. |
| `TESSERA_TELEMETRY__ANSI` | No | `false` | Disables terminal color escape sequences in hosted logs. |
| `TESSERA_AUTH__ADMIN_PASSWORD` | No | Automatically generated | Sets the initial administrator password. If omitted, Tessera generates one and prints it once in the deployment logs. |
| `TESSERA_SECURITY__SECRET_KEY` | Recommended | Generated in `/data/secret.key` | Encrypts stored credentials and storage backend configuration. Set a stable random value and never change it after deployment. |
| `TESSERA_SERVER__MAX_BODY_BYTES` | No | `104857600` | Maximum request body size in bytes. Use this to limit the largest upload accepted by the server. |
| `TESSERA_FEATURES__DOCS` | No | `true` | Enables the OpenAPI documentation endpoint. Set to `false` for a public production instance. |

Recommended values:

```env
TESSERA_SERVER__HOST=0.0.0.0
TESSERA_SERVER__PORT=12923
TESSERA_DATABASE__URL=sqlite:///data/tessera.db?mode=rwc
TESSERA_SERVER__DATA_DIR=/data
RAILWAY_RUN_UID=0
TESSERA_TELEMETRY__FORMAT=json
TESSERA_TELEMETRY__ANSI=false
TESSERA_FEATURES__DOCS=false
```

## Persistent Storage

Add a Railway volume mounted at `/data`. The volume is required for production use. Without it, the SQLite database, uploaded files, generated encryption key, and other local data will be lost when the service is redeployed or restarted.

## Administrator Access

On the first deployment, Tessera creates an administrator account. If `TESSERA_AUTH__ADMIN_PASSWORD` is not set, Tessera generates a password and prints it once in the deployment logs. Store the password securely and change it after signing in.

For predictable deployments, set `TESSERA_AUTH__ADMIN_PASSWORD` in Railway's variable configuration instead of committing it to source control.

## Security Recommendations

- Set a strong `TESSERA_SECURITY__SECRET_KEY`.
- Attach a persistent `/data` volume.
- Set `TESSERA_FEATURES__DOCS=false` unless the API documentation is required.
- Configure upload size and storage limits.
- Consider disabling anonymous uploads.
- Use PostgreSQL for multi-instance deployments.
- Configure an external malware scanner for untrusted uploads.
- Never commit passwords, database credentials, or secret keys to Git.

## Optional Database Configuration

SQLite is the simplest option for a single Railway service:

```env
TESSERA_DATABASE__URL=sqlite:///data/tessera.db?mode=rwc
```

For a larger or multi-instance deployment, use PostgreSQL:

```env
TESSERA_DATABASE__URL=postgres://USER:PASSWORD@HOST:5432/DATABASE
```

Keep database credentials in Railway variables or Railway references. Do not place them in the repository.

## Troubleshooting

### The service is not reachable

Confirm that `TESSERA_SERVER__HOST=0.0.0.0` and `TESSERA_SERVER__PORT=12923` are configured, and that the Railway service domain targets port `12923`.

### Data disappeared after redeployment

Verify that a Railway volume is mounted at `/data`.

### The administrator password is unavailable

Check the first deployment logs. If a password was generated automatically, Tessera prints it only once. For a new installation, set `TESSERA_AUTH__ADMIN_PASSWORD` before deploying.

## License

Tessera is licensed under the AGPL-3.0 license.
