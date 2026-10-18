# Settings the API can change

Sources: [config.rs](src/config.rs), [settings.rs](src/settings.rs), [api/settings.rs](src/api/settings.rs), [crates/tessera-core](../../crates/tessera-core/src/)

> The authority for this list is what `settings::keys()` returns (the keys come from walking a serialised `Config::default()`).
> Update this file when you add a config field.

## API

- `GET /admin/settings` lists every setting, the ones that cannot be changed included (marked `editable: false` with a `refusal` reason).
- `PUT /admin/settings` changes settings: `changes: {key: value}`. `value: null` clears a key, so it falls back to the environment variable or the default.
- `POST /admin/settings/preview` shows what a change would delete (shortening a retention period, say) without applying it.

Changing settings needs the `SettingsWrite` permission (`preview` and `GET` need `SettingsRead`), and `features.admin` has to be on.

**Validated as a batch.** The changes in one `PUT` are validated together, and one invalid key sends the whole batch back, so send only the fields that actually changed.

## Three reasons a setting cannot be changed

| Mark | Meaning | In the panel |
|---|---|---|
| `FIXED_AT_BOOT` | Decided at start-up; a change would not take effect | Readable, writes refused |
| `SECRET` | Carries a credential that does not belong in the settings table | Neither readable nor writable; environment variable only |
| `SEALED` | Carries a credential, but an operator has good reason to set it from the panel | **Writable, never readable** (stored encrypted) |

### `FIXED_AT_BOOT`

| Key | Why |
|---|---|
| `server.host` / `server.port` | The listener is already bound |
| `database.*` | Reading the settings table needs the connection pool first |
| `telemetry.*` | The log subscriber is installed before anything else |
| `server.cors_allowed_origins` | Baked into the middleware when the router is built |
| `server.max_body_bytes` | Same, and only half of its logic would follow a change (see below) |
| `server.assets_dir` | The SPA is read once at start-up |
| `server.data_dir` | The key file and the first backend are put here on first start; moving it later leaves both behind |
| `security.secret_key` | Builds the sealer; changing it makes every encrypted setting unreadable |
| `features.docs` | Decides at router build whether the route is mounted |

### `SECRET`

| Key | Why |
|---|---|
| `auth.admin_password` | Used once, on first start; storing it serves no purpose |
| `database.url` | The URL carries the credentials |
| `security.secret_key` | Opens every encrypted setting |

### `SEALED`

| Key |
|---|
| `auth.oidc.client_secret` |
| `auth.github.client_secret` |
| `auth.discord.client_secret` |
| `mail.username` / `mail.password` |
| `scan.authorization` |

The panel can write these and is told whether each has been set, but never gets the value. A form for them has to work as "**empty = leave unchanged, filled in = overwrite**", not the usual load-the-current-value-and-edit.

They are encrypted with `security.secret_key`. When that is not set, the key written to `server.data_dir/secret.key` on first start is used, so a write always succeeds.

---

## `appearance.*`

What a signed-out visitor sees, and whatever a signed-in user has not chosen for themselves. Written into `index.html`, so the first paint already has the right colours.

- `mode: "light" | "dark" | "system"`: default `system` (follows `prefers-color-scheme`)
- `theme: "default" | "graphite" | "sepia" | "matsu" | "cosmic" | "slate" | "vintage"`: the neutral palette (background, text, borders). Default `default`
- `accent: "violet" | "rose" | "orange" | "amber" | "green" | "teal" | "blue"`: the emphasis colour (buttons, links, focus ring). Default `orange`

Three keys rather than one object: the panel's key list is read from the structure, and a nested object would count as a group of settings, not one setting. It also means each value is validated by parsing, so an unknown accent is refused down the same path as anything else unknown.

## `auth.*`

### Basics

- `registration: "open" | "invite" | "closed"`: who can sign up. Default `open`
- `default_role: string`: the role a new account gets, **by name, not id**. Default `member`. Checked at start-up; the server will not start if the role does not exist
- `session_ttl_hours: u64`: default `720` (30 days)
- `login_history_days: u64`: how long an ended session stays in the sign-in history. Default `90`. It answers "has anyone else been in?", so it outlives the credential
- `cookie_secure: "auto" | "always" | "never"`: when the session cookie is marked `Secure`. Default `auto`
  - `auto` marks it only over HTTPS, so it works both on a plain-HTTP LAN and behind public TLS
  - `always` **silently breaks sign-in** anywhere that is not HTTPS all the way (the browser accepts the response and drops the cookie; the user sees a successful sign-in and nothing happens)
- `admin_username` / `admin_email: string`: the administrator created on first start
- `admin_password: string` (SECRET): empty = generated and printed once

### `auth.password.*`

Length is the main rule; the composition rules are all off by default. Following NIST 800-63B, those rules only produce `Password1!`, and length is worth more than character classes. They exist for operators who answer to an internal policy that still demands them.

- `min_length: usize`: default `8`, minimum 8
- `max_length: usize`: default `256`, maximum 1024. A limit is needed because an unbounded password field lets an unauthenticated request make the server do any amount of work
- `require_mixed_case: bool`: needs both upper- and lower-case letters. Default `false`
- `require_digit: bool`: needs a digit. Default `false`
- `require_symbol: bool`: needs a symbol. Default `false`. **A space is not a symbol**, or every passphrase would pass

Composition rules apply only when **choosing a new password** (sign-up, change, reset). Sign-in does not check them, so turning one on does not lock existing accounts out. Character classes follow Unicode (see the table in `PasswordPolicy`'s comments, with the matching front-end regexes), so a password in any language is held to the same standard.

### `auth.pwned.*` (breached-password check, off by default)

- `enabled: bool`: check Have I Been Pwned when a new password is set. Default `false`
- `url: string`: the range API. Default `https://api.pwnedpasswords.com`. Can point at a self-hosted implementation

The front end can check while the user types: `GET /api/v1/auth/pwned/{prefix}` takes the first 5 characters of the SHA-1 and returns the suffixes under that prefix that really were breached (padding removed); the comparison happens in the browser. `account_password_breach_check` in `/capabilities` says whether this instance has it on. When the outside service cannot be reached this answers 503, and the check on submit still runs.

Only the first 5 characters of the password's SHA-1 leave the machine, and the comparison is local (k-anonymity): neither the password nor its full hash is sent. If the service cannot be reached (timeout, no connection, a non-200) the password is allowed and one line goes to the log. Sign-up, password change and reset must not stop because someone else's service is down. All three are checked.

### `auth.hashing.*` (Argon2id)

- `memory_kib: u32`: default `19456` (19 MiB, the OWASP recommendation). **The parameter that matters most**: memory cost is what makes GPU cracking expensive, and it is also the memory a sign-in spike uses, times the number running at once
- `iterations: u32`: default `2`
- `lanes: u32`: default `1`

Changing these **does not invalidate existing passwords**. The parameters are stored in each hash and read back from it when verifying, so they only affect new hashes.

### `auth.throttle.*`

Nine counters protect different things. Each is `{max_attempts, window_secs}`.

| Key | Counted by | Protects |
|---|---|---|
| `login` | Account + source | Failed sign-ins |
| `source` | Source address | Anything expensive. **Deliberately generous**: addresses are shared, and blocking one blocks everyone behind it |
| `lookup` | Source | Looking up codes that do not exist. **Short codes are only as safe as this** |
| `reset` | The email asked about | Sending password-reset mail |
| `registration` | The email asked about | Sending sign-up confirmation mail |
| `verification` | The receiving email | Email verification mail. **Not shared with `reset`**, or changing your own address to someone else's would use up their reset allowance |
| `share` | Code + source | Wrong share passwords |
| `speed` | Source | Speed tests. Counts **every test**, not failures: it limits how much bandwidth one address can use in an hour |
| `pwned` | Source | Breached-password checks while typing (`GET /auth/pwned/{prefix}`). Every query counts; default 30 a minute. **Not shared with `source`**, so typing a password does not use up the sign-in allowance of the same address |

**Neither value may be 0**; start-up refuses it. Zero attempts locks everyone out (on `source`, the whole machine), and a zero-second window never blocks anything, so it looks on while being off. To loosen a limit, raise `max_attempts`.

- `max_tracked_keys: usize`: how many keys are tracked before a sweep, at most `100_000`. **This is a memory limit, not a security setting**: every key is a string an unauthenticated stranger chose, and this number decides how much memory they can make the process hold. Shared by all nine

### `auth.oidc.*`

An empty `issuer` turns the whole feature off: no routes mounted, nothing advertised.

The provider decides **who can sign in**; Tessera decides **what they can do** (a new account gets `auth.default_role`, and an administrator adjusts it afterwards, as for an account that signed up with a password). The group claim is used only for `required_group` and is **never mapped to a role**.

- `issuer: string`: the provider's issuer URL; everything else comes from its discovery document. Must be https, and must equal the `iss` claim exactly as a string (the spec requires it)
- `client_id: string`
- `client_secret: string` (SEALED)
- `scopes: string[]`: `openid` is added if missing. **`email` is required**: a sign-in without an email is refused rather than the column being made nullable
- `required_group: string`: **only members of this group can sign in. An empty string = anyone the provider authenticates gets in**
- `group_claim: string`: the claim that holds the groups. Authentik and Authelia use `groups`, Keycloak `realm_access.roles`, some LDAP bridges `memberOf`. Dots select a nested field
- `display_name: string`: the sign-in button's text. Empty = the issuer's host
- `link_by_email: "never" | "when_verified" | "always"`: whether a provider identity can attach to an existing account with the same email. Default `never`
  - `when_verified`: only when the provider says `email_verified` is true
  - `always`: whatever the provider says. For providers such as Synology, Entra ID and Authentik that omit `email_verified` or get it wrong. **If the provider lets anyone sign up with any address, this lets a person take over an account here by typing someone else's address.** Only for an IdP whose accounts you control
- `allow_registration: "inherit" | "always" | "never"`: whether a first sign-in through this provider creates an account. Default `inherit` (follows `auth.registration`, invitations included). `always` ignores `auth.registration` and needs no invitation, for a site whose accounts all live in its own IdP with password sign-up closed. `never` lets only existing accounts sign in

> **Leaving `required_group` empty is a real security problem.**
> A provider that serves several applications authenticates everyone who has an account with it, not the people who should get into your site.
> Configure Google and leave `required_group` empty, and any Google account in the world can sign in.

### `auth.github.*` / `auth.discord.*`

The two have the same fields. They are separate from OIDC because neither publishes a discovery document: the endpoints are fixed in the adapter, and all that is left here is the pair of values that identify this site to them.

- `client_id: string`
- `client_secret: string` (SEALED)
- `scopes: string[]`: **not decoration**. Without `user:email` GitHub withholds the verified address, and Discord does the same without `email`. Narrowing them makes every sign-in fail with `no_address`. The default is a working set, checked at start-up
- `base_url: string`: for GitHub Enterprise Server (self-hosted on your own domain). Empty = github.com. **Discord ignores it**
- `display_name: string`: the button's text. Empty = the provider's name
- `link_by_verified_email: bool`: default `false`. For the same reason as OIDC: an account on either takes a minute to create, so open sign-up plus matching by email gives other people's accounts away
- `allow_registration: "inherit" | "always" | "never"`: as for OIDC. **Neither has groups to filter on, so `always` means any GitHub or Discord account in the world can open an account here**

## `share.*`

- `code_length: usize`: characters in a retrieval code. Default `8`. Eight Crockford base32 characters ≈ 1.1×10¹² combinations; the 5 characters some similar services use give 33 million, which a public endpoint can be walked through. **Short codes only hold up because `throttle.lookup` exists**
- `inline_threshold_bytes: usize`: how much of an uploaded file is buffered in memory before the multipart upload to object storage starts. Default `64 KiB`. It does not decide where content is stored: files always go to object storage, text shares always go to the database.
- `max_text_bytes: usize`: the limit for a text share. Default `1 MiB`. Text is stored in the database, so this is **the largest single row, and the most memory one upload can take**: something received whole cannot be streamed
- `ticket_ttl_hours: u64`: the longest a download authorisation lives. Default `24`. One authorisation uses up one retrieval; the ticket it yields can be reused (Range requests, resuming, retrying after a dropped connection). **Without that split, one interrupted download would burn a one-time share.** This is a ceiling, not the usual lifetime: most tickets die of idleness first
- `ticket_idle_hours: u64`: how long a ticket can sit unused. Default `1`. Idleness rather than a fixed lifetime: a ticket is a string and can be passed on, and a fixed lifetime would have to cover the slowest line fetching the largest file, which makes a ticket worth passing on. Every use pushes it back
- `default_expiry_hours: u64`: lifetime when none is given. Default `168` (7 days). `0` = use `max_expiry_hours`. **This decides disk usage more than any quota**: what sits on disk is roughly upload rate × retention
- `max_expiry_hours: u64`: the longest lifetime that can be asked for. Default `720` (30 days). `0` = no limit
- `retain_after_death_hours: u64`: how long an expired or used-up share is kept before it is really deleted. Default `168` (7 days). Keeping it lets the link say "this has expired" rather than "no such code"; only the first is worth asking the sender again about. `0` = delete at the next sweep
- `upload_session_ttl_hours: u64`: how long an unfinished upload is kept. Default `24`. **Its bytes count against the quota the whole time.** Long enough for a laptop closed overnight to resume, short enough that dead sessions do not pile up

> **With S3, also set a lifecycle rule.**
> The sweep only aborts the unfinished uploads it can find, a failed abort leaves its parts behind, and S3 never cleans up abandoned multipart uploads by itself.
> Add an `AbortIncompleteMultipartUpload` rule a few days longer than `upload_session_ttl_hours`.

### `share.access_log.*`

- `enabled: bool`: record every retrieval. Default `false`
- `retention_days: u64`: how long records are kept. Default `30`. **May not be 0 while enabled**

Off by default on purpose. `view_count` is a number and says nothing about where visitors are; an access record is an address and a browser string, about someone who received a link and agreed to nothing. That is the operator's decision to make, not something to inherit by default, and an instance that turns it on should say so on its pages.

`retention_days` refuses 0: reading 0 as "forever" is the wrong default for a setting about other people's data.

`GET /admin/shares/{id}/access-log` carries `enabled` in `meta`, so the front end can tell "nobody downloaded it" from "nothing is being recorded". Both have an empty `data` array.

## `site.*`

- `title: string`: the tab title, and the site name in link previews. Default `Tessera`. Not blank, at most 100 characters
- `description: string`: the text a preview shows for a link to the site itself. Default empty (empty = the meta tag is left out). At most 300 characters
- `language: "en" | "zh-TW" | "zh-CN" | "ja"`: the instance's default language. Default `en`
  - Share link previews (meta tags and the `og.png` card) are written in it. The site's language rather than the request's, because crawlers such as Discord's send no language preference
  - The web app opens in it for a visitor with no saved choice (cookie) **and** whose browser asks for none of the four languages. The order is cookie → browser language → `site.language`, so a visitor whose browser says `ja` still gets Japanese on an `en` instance. Written into `index.html` as `data-default-locale` on `<html>`; the dev server has no such attribute and falls back to the compiled-in `zh-TW`

`title` and `description` are written into `index.html` before it is sent, as `appearance.*` is, so the first paint is right without waiting for a script. A share page uses the share's own description; this does not override it.

**No SEO fields** (`keywords` and the like).

## `speed.*` (speed test, off by default)

- `enabled: bool`: whether the speed test endpoints exist. Default `false`
- `anonymous: bool`: whether signed-out visitors can use it. Default `false` (signed-in only)
- `max_bytes: u64`: the most one test moves (each direction). Default 25 MiB

The endpoints are `GET /api/v1/speed/download?bytes=N` and `POST /api/v1/speed/upload`; the bytes sent mean nothing and the bytes received are thrown away. When it is off, or not open to anonymous visitors, whoever should not see it gets a 404, the same as on a site without the feature.

**This is a tap**, so all three limits are in place: off by default, signed-in only by default, a cap per test, plus the per-source count in `auth.throttle.speed`. `speed_test` in `/capabilities` tells the front end whether this caller can run one.

## `security.*`

- `secret_key: string` (SECRET + FIXED): seals every setting that is stored in the database and has to be read back. 64 hex characters. Empty (the default) = use the key generated on first start and stored at `server.data_dir/secret.key` (mode 0600). Set it yourself (`openssl rand -hex 32`) to keep the key out of the data directory's backups

It seals the keys in the `SEALED` table above: the OIDC, GitHub and Discord client secrets, the mail credentials, the scanner's authorization, and the credentials of storage backends added from the panel. **It has nothing to do with object storage.** It used to be called `storage.secret_key` because backend credentials were the first thing that needed it; the name misled, so it was changed.

The key can only come from the environment, so a database backup never holds both the lock and the key. Lose it and every encrypted setting has to be entered again.

## `scan.*`

Malware scanning, off by default. See `docs/scanning.md`.

- `url: string`: the scanner. An empty string = no scanning. `clamd://host:3310` (the ClamAV daemon) or `http(s)://…` (receives the file in a POST, answers `{"clean": bool, "threat": "…"}`)
- `authorization: string` (SEALED): the `Authorization` sent to an HTTP scanner

A share cannot be downloaded until it has been scanned (`423` with `Retry-After`). If the scanner cannot be reached the share is let through and scanned once it is back; a share found infected is disabled. Encrypted shares and files over clamd's size limit are not scanned.

## `storage.*`

Where files are stored is not a setting. Each backend is a row in the `storage_backends` table, managed from `/admin/storage/backends` in the panel or with `tessera-server backends`. A brand-new instance writes a `default` row on start (a local directory, `server.data_dir/blobs`, taking new uploads), which can then be changed, retired or removed like any other.

- `max_total_bytes: u64`: the most content the whole instance holds. `0` = no limit. Default `0`
- `split_across_backends: bool`: whether one file can be spread over several backends. Default `false`
- `reconcile_interval_hours: u64`: how often object storage is reconciled with the database. Default `24`; `0` turns it off
- `reconcile_grace_hours: u64`: how old an object nothing refers to must be before reconciliation deletes it. Default `24`

> **`max_total_bytes` unset with `anonymous_upload` on (both are the defaults) = an unlimited network drive open to anyone who can reach it**,
> and the first sign is a full disk and failing writes.
> Under concurrency it is approximate: it can be overshot by up to "uploads in flight × `server.max_body_bytes`", so leave the same headroom on the disk.
> `share.default_expiry_hours` is usually the better lever: shortening it lowers steady-state usage without refusing anyone.

With `split_across_backends` on, a file larger than any single account can be stored (five free 10 GB buckets hold a 40 GB file; with it off the upload gets `507`). The price is that **a split file has no direct link**: a presigned URL points at one object, so the bytes go through this server or are joined in the browser, and on free tiers that uses upload and download bandwidth both. A file is only split when no single backend can hold it; see `docs/splitting.md`.

Reconciliation is separate from the hourly sweep because it lists the whole store, while the sweep is a DELETE on an indexed column. **Most object stores bill listings as separate API calls**: lengthen `reconcile_interval_hours` when the request budget is tighter than the byte budget, shorten it on local disk.

`reconcile_grace_hours` is deliberately generous. The object is always written before its row, so every upload looks orphaned for a moment. Reclaiming stray bytes a day late costs nothing; deleting a file just before its row lands loses it.

## `server.*`

- `request_timeout_secs: u64`: the wall-clock budget for an ordinary request. Default `30`. **Not applied to upload routes**
- `upload_stall_secs: u64`: how long an upload can go without receiving a byte before it counts as dropped. Default `30`
- `part_size_bytes: usize`: how much is gathered before a part is sent to object storage. Default `8 MiB`
- `public_url: string`: the site's absolute public URL. Default empty
- `trusted_proxies: string[]`: proxies in front that can be trusted, as CIDRs or bare addresses. Default `[]`
- `trusted_proxy: bool`: **deprecated**, use `trusted_proxies`. Default `false`
- `host: IpAddr` / `port: u16` (FIXED)
- `max_body_bytes: usize` (FIXED): the largest request body
- `cors_allowed_origins: string[]` (FIXED): empty = same origin only, the right default for the single-binary deployment
- `assets_dir: string` (FIXED): serve the front end from disk instead of the embedded bundle. Pointing it at a directory without `index.html` stops the server (rather than silently falling back to an old build)
- `data_dir: string` (FIXED): where the instance keeps what it writes. Default `data`. Only used on first start: for `secret.key` when `security.secret_key` is unset, and for the `blobs` backend when the table is empty

**Why uploads get no wall-clock budget.** One number cannot serve both sides. A 5 GB file over 20 Mbit/s takes 2400 seconds, and that would also be how long someone who connects and sends nothing could hold a slot: three orders of magnitude apart. So `upload_stall_secs` asks a different question: is the other end still sending?

**The trap in `part_size_bytes`.** S3 refuses any part but the last that is under 5 MiB; Cloudflare R2 also requires every part to be the **same** size (so on R2 this should equal the minimum, as tusd does). Garage checks neither, so CI does not catch a mistake here.

**`public_url` is only needed for link previews, and it must be absolute**: `og:image` is fetched by a crawler on another machine. When empty it is derived from each request's `Host` and forwarded scheme, which breaks in exactly the cases it is meant for: a reverse proxy that rewrites `Host`, or a container that only sees `localhost:8080`. **A blank preview card means this needs setting.** A trailing slash is removed.

**`trusted_proxies` trusts nothing by default.** `X-Forwarded-For` and `-Proto` are set by whoever spoke to us last, which on a direct connection is the caller. Trusting them unconditionally lets the caller choose its own source address (which every throttle here counts by) and claim HTTPS over a plain connection. For a proxy on the same machine use `["127.0.0.1", "::1"]`; for a container network, `["10.0.0.0/8"]`.

The deprecated `trusted_proxy: true` means "everything that connects is a proxy", so anyone who can reach this port directly can forge the headers. It is kept so an upgrade does not lock the operator out, and start-up warns about it.

> **`server.max_body_bytes` is the worst of the FIXED ones.**
> Half of it *does* follow a change: the hand-written check on the upload path reads the current setting, and the `DefaultBodyLimit` layer does not.
> An operator who raises it finds large files still refused by a layer whose number no longer matches.

## `features.*`

Run-time switches. **A feature that is off has no routes mounted at all**, so calling it gets a plain 404 rather than a 403 that confirms it exists. Turning one off does not make the binary smaller; that is what cargo features (`postgres`, `storage-s3`) are for.

`GET /api/v1/capabilities` reports what is actually in effect: the SPA cannot tell from the static contract what an instance has turned on.

- `accounts: bool`: sign-up, sign-in and sessions. Default `true`. Off = no accounts at all, for one person behind a VPN who only wants a drop point for files
- `admin: bool`: the admin endpoints, which the whole `/admin` panel depends on. Default `true`
- `anonymous_upload: bool`: whether uploading works without a session. Default `true`. **Deliberately independent of `accounts`**: wanting an admin panel is not wanting to make everyone register
- `encryption: bool`: allows end-to-end encrypted shares. Default `false`. Turning it off only stops new ones; shares already encrypted can still be downloaded
- `docs: bool` (FIXED): the `/docs` browser. Default `false`. The page is a few lines of HTML that load a renderer from a CDN; **the server never fetches it, the browser does**. That saves about 11 MB of binary (23.8 MB → 12.5 MB). `openapi.json` is served either way

## `mail.*`

An empty `host` = completely off, which is the default and the common case. On a single-user instance, `reset-password` run on the machine gets you back into a forgotten account.

Tessera does not act as a mail server because home connections cannot deliver mail: most ISPs block port 25, and receivers reject residential addresses whatever their SPF and DKIM say. A relay is expected (Resend, Brevo, SES, Fastmail or your company's own).

- `host: string`: the SMTP host. An empty string turns off everything that sends mail
- `port: u16`: default `587` (submission + STARTTLS); `465`, implicit TLS, is common too
- `tls: "auto" | "starttls" | "implicit" | "none"`: how the connection to the relay is encrypted. Default `auto` (implicit on `465`, STARTTLS on any other port). `none` does not encrypt at all, for a relay on the internal network such as a local postfix, a relay container or Mailpit; every address the host resolves to must then be private or loopback, or the mail is not sent
- `username: string` (SEALED): Resend wants the literal string `resend` here
- `password: string` (SEALED): for most relays, the API key
- `from: string`: the `From:` address. Its domain must be verified with the relay
- `public_url: string`: the base for links in mail
- `reset_ttl_minutes: u64`: how many minutes a password-reset link works. Default `30`, at most `1440` (24 hours)
- `registration_ttl_hours: u64`: how long a sign-up confirmation link works. Default `24`, at most `168` (7 days). Email verification links use it too

`host`, `from` and `public_url` are set all together or not at all; a partial setup is refused.

**`mail.public_url` deliberately comes from configuration, not the request's `Host` header**: a reset link built from a header an attacker controls would point at their server and hand them the token.

The two lifetimes differ because the links grant different things. A reset link **is a credential**, so it is short, just long enough to survive a mail queue. A sign-up link grants less: the account it creates uses the password the applicant already chose, so a stolen link does not get anyone in.

## `og.*`

- `layout: string`: an operator's own HTML for the preview card. Default empty (the built-in card)

Once set, it replaces the card for **every share**. What each share may reveal is still decided elsewhere, so on a locked share's card `{{size}}` is empty, because the value is empty, not because the layout guards against it.

**Only `div`, `span` and the drawable part of inline `<svg>` are allowed, styled with `style` or `tw`, and nothing may point at a resource outside this server**; anything else is refused. The full list is in `og::layout`, the background in `docs/link-previews.md`. The panel's preview card page shows all four states as you edit.

## `database.*` (all FIXED, environment variables only)

- `url` (SECRET), `max_connections` (default `8`), `acquire_timeout_secs` (default `10`), `run_migrations` (default `true`)

## `telemetry.*` (all FIXED)

- `format: "pretty" | "json"`: `pretty` is human-readable and multi-line, for local development; `json` is one object per line, for log shippers
- `filter: string`
- `ansi: bool`
