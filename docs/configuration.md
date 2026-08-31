# Configuration

There is no configuration file. Values come from three places.

Precedence, lowest to highest:

1. Built-in defaults
2. Environment variables. `TESSERA_` prefix, `__` separates nesting (`TESSERA_SERVER__PORT=8080`)
3. The `settings` table, written by the admin panel

Code: `config.rs` (the shape and validation), `settings.rs` (what the panel may change).

## The rule the split enforces

**Anything stored in the database takes effect without a restart.**

The panel never has to say "saved, now restart the server", because a setting that would need one is not offered.

That is a promise about the list, not about the settings. A setting is excluded only when the running process cannot pick it up, and there are three reasons for that:

| Excluded | Why |
| --- | --- |
| `server.host`, `server.port` | The socket is already bound |
| `database.*` | The pool is already built, and reading this table needs it |
| `telemetry.*` | The log subscriber is installed once, before anything else |

Those live in the environment, which is where a deployment sets them anyway.

## Two kinds of secret

A stored secret is plaintext in a table and comes back out of whatever endpoint reads settings. That is true of a value stored as it stands, and it is why most credentials are not offered.

Some are, because the alternative is worse. `mail.host` is changeable and `mail.password` was not, so configuring a relay meant doing half of it in the panel and half in the environment, then restarting for the half the panel could not do.

| | Never stored | Stored, encrypted |
| --- | --- | --- |
| Which | `auth.admin_password`, `database.url`, `storage.url`, `storage.secret_key` | `mail.username`, `mail.password`, `auth.oidc.client_secret` |
| Why | Read once at start-up, or carries credentials inside a URL that is also a connection string | An operator has every reason to set it from the panel |
| The panel sees | nothing | whether one is set, never the value |

The second column is sealed with `storage.secret_key`, which comes from the environment, so a database dump on its own opens nothing. Without a key, storing one is refused rather than written in plaintext, and the refusal names what to set.

`storage.secret_key` can never be in the second column itself: it is what opens the others.

A stored value carries a `sealed:` marker. Without one, ciphertext and plaintext are indistinguishable both to somebody reading the table and to the code reading it back, and a value that will not open would be as likely to be a corrupt row as a changed key. Those send an operator to different places.

## The settings list is derived, not written down

`settings::keys()` walks `Config::default()` serialised to JSON and collects the dotted paths.

A hand-maintained list goes stale the first time someone adds a field and does not think of it, and the symptom would be a setting the panel silently cannot see.

**Consequence for anyone adding a field:** adding it to `Config` makes it settable from the panel automatically. If it should not be, add it to `SECRETS` or `FIXED_AT_BOOT`.

## Validation

`Config::load` validates everything. A bad log filter or an unsupported database URL fails at start-up, not on the first request that touches it.

A bad value from the _table_ is dropped instead of fatal; see `Config::load_with_overrides`. The panel must not be able to make an instance unbootable.

Saving through the panel validates the whole resulting config as a set, and stores nothing unless it loads. Settings that only make sense together, such as turning the access log on and setting its retention, never have a moment where one applies and the other does not.

Because validation lives in `Config::validate`, the file, the environment and the panel are held to the same rules. A value the panel refuses cannot be smuggled in by setting an environment variable.

## Live reload

The running `Config` sits behind an `ArcSwap`. `AppState::config()` reads it.

Nothing should hold the result across an `await`, or it will act on a configuration that has since been replaced.

Middleware that reads a setting has to read it per request rather than being built from it once. That is why the timeout is applied in a handler-aware layer rather than a `TimeoutLayer` built at startup; see `api/timeout.rs`.

## Consequences preview

`POST /api/v1/admin/settings/preview` reports what a change would destroy before it is applied, such as shortening a retention window so that existing shares would be swept.
