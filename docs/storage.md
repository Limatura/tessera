# Storage backends

Content past `share.inline_threshold_bytes` goes to an object store. Everything reaches it through `object_store`, so any backend that crate supports is a candidate. They do not all support the same operations, and the differences matter.

Code: `storage/mod.rs`, `storage/placement.rs`, `storage/first_write.rs`, `db/repository/storage_backend.rs`.

## What a backend is here

Every backend is a row in the `storage_backends` table, with its URL and options sealed. There is no other kind. Rows are added, changed and removed at runtime through `/api/v1/admin/storage/backends` or `tessera-server backends`, and the pool is rebuilt on the spot.

A fresh instance has no rows, so start-up writes one (`storage::seed`): `default`, a local directory at `blobs` under `server.data_dir`, marked as where new uploads go. Only into an empty table, so an operator who replaced it with a bucket does not get it back on the next start. Nothing about it is special after that; it can be repointed, retired or removed like any other.

`storage::Pool` holds the open stores (`open`, by name) and the rows they came from (`configured`). A row that will not open is logged and left out of `open`, and shares on it answer `503` until it is fixed; the others keep working.

## Whether a backend is answering

Opening a store only parses its URL and options; nothing is sent until the first request. So whether a backend works is learned by calling it, and the admin listing shows the last call's outcome: `reachable`, `last_checked_at` and `last_error` (`storage::health`).

- **In passing.** Every store in the pool notes how each call to it went: an upload, a download, a deletion, the reconciliation's listing. A backend in use costs nothing extra to watch. "No such object" counts as an answer; a refused connection, a timeout, or rejected credentials do not.
- **When added or changed.** The backend is asked before the response: a listing of `shares/`, then, unless it is read-only, writing and removing a small object there, since a bucket can be readable with credentials that may not write. One that fails is still stored, shown as unreachable with the reason: the store may simply not be up yet.
- **When idle.** A background task lists `shares/` on any backend not called for fifteen minutes, and every five minutes on one whose last call failed, so it shows as back soon after it is. Never a write: this runs unattended.

The probes list, so the credentials need permission to list the bucket, which the reconciliation needs anyway. What is learned is kept in memory, so after a restart a backend shows as reachable until the first pass, a moment after start-up.

Every `file_segments` row names the backend it lives on. A file stored before backends were recorded reads as `default`, because there was nowhere else it could have been.

Deleting a share, by hand or by the sweep, removes each object from the backend its segment names, and the daily reconciliation lists every open backend for orphans, not only the default one.

## Two different capability questions

`storage::Stores` carries two flags, and they are not the same thing.

|  | What it means | False for |
| --- | --- | --- |
| `multipart` | Implements `MultipartStore`, a part-level view whose upload ID outlives the request | `LocalFileSystem`, HTTP/WebDAV |
| `put_multipart` | `ObjectStoreExt::put_multipart` works, streaming a body out in pieces within one request | HTTP/WebDAV only |

`LocalFileSystem` is the case that shows they differ: no part-level view, but `put_multipart` works fine.

Consequences:

- **No `multipart`.** Resumable uploads use the appending sink instead (`resumable.md`).
- **No `put_multipart`.** A file too large to hold in memory has nowhere to go, so the placer skips that backend for large files and answers `413`.

WebDAV's only upload verb is a whole-body `PUT`. `object_store` has no streaming put yet; this is upstream `apache/arrow-rs-object-store#281`.

## Sending browsers to the bucket

Downloads (`retrieval.md`) and upload parts (`resumable.md`) can go between the browser and the store directly, without crossing this server. It is off unless an operator turns it on, per backend: `direct_access` on `POST /api/v1/admin/storage/backends`, or `PATCH /api/v1/admin/storage/backends/{name}`.

`PATCH` exists so the switch can be flipped without sending the credentials again; they are sealed and never shown.

**Turn it on only if a browser can reach the store.** A bucket on a home network answers this server and nobody else, and nothing here can tell, because the network that matters is the downloader's. A download sent there fails on a page this server never sees.

Only S3 and GCS can be sent to. On anything else the switch changes nothing.

### Uploads, and joining pieces, need CORS

A browser writing upload parts directly, or fetching the pieces of a split file to join them (`retrieval.md`), is making a cross-origin request, and the bucket decides whether to allow it. Two things are needed:

- `PUT` allowed from the site's origin
- `ETag` in the exposed headers, or the browser cannot read the value `complete` needs

On S3 and most S3-compatible stores:

```json
[
  {
    "AllowedOrigins": ["https://files.example.com"],
    "AllowedMethods": ["PUT", "GET"],
    "AllowedHeaders": ["*"],
    "ExposeHeaders": ["ETag"],
    "MaxAgeSeconds": 3600
  }
]
```

Without it, uploads still work; they just go through this server.

An ordinary download needs none of this: following a redirect from a link is a navigation, not a cross-origin request.

## Choosing a backend

`placement::choose` is best-fit: the fullest backend that still has room.

`rclone` ships both this and its opposite. Spreading across the emptiest backend balances load and leaves every backend with a gap. Filling the fullest that still fits concentrates the leftover into one place.

The audience is someone pooling free tiers, where the scarce thing is bytes, not throughput.

`choose` never splits a file. If nothing fits, that is a refusal. `storage::split` is the other answer, and it is off by default.

## First write into a fresh prefix

Object stores have no directories. WebDAV does, and does not create them on `PUT`: the write answers `409`, and `object_store` walks up with MKCOL and retries **once**.

Several uploads arriving together into a fresh backend each hit the `409`, each start creating `shares/YYYY/MM`, and the one whose single retry lands while a sibling's MKCOL is in flight has nothing left to try.

Measured against `rclone serve webdav`: a four-test suite run in parallel was reliably flaky. `storage::first_write` serialises the first write per prefix and remembers which prefixes exist.

## What CI proves

The object-store job runs the full suite against eight implementations on every PR.

| Store | Why it is in the list |
| --- | --- |
| Garage | What the self-hosting audience runs. Deliberately permissive: enforces neither the 5 MiB part floor nor equal part sizes |
| RustFS | Stricter than Garage, and where the self-hosting audience is moving |
| MinIO | The reference S3 implementation. Its open-source repo was archived in early 2026 |
| SeaweedFS | Needs `-s3.config` with an identities file, or every write answers `PermissionDenied` regardless of credentials |
| VersityGW | Another S3 gateway shape |
| s3proxy | S3 in front of other stores |
| Azurite | Azure, not S3. Signs with Shared Key |
| rclone WebDAV | The one without `put_multipart` |

Plus an opt-in run against a real provider when the repository has credentials configured.

Because Garage enforces neither part-size rule, a mistake in the part arithmetic would pass there. That is why `upload/sink.rs` pins it with unit tests too.

### Traps found by running these

- **SeaweedFS** without an identities file rejects every write with `PermissionDenied`, whatever credentials are sent. This is why it looked incompatible at first.
- **Azurite** needs `LC_ALL=C` when signing with `openssl` in a shell script. Without it `date` emits a localised day name and the request answers `403` with no explanation.
- **WebDAV** needs `allow_http=true` and MKCOL for parent collections.
- **A store with no disk space** marks its volumes read-only and answers writes with an error the gateway reports as a plain `500`. The test failure says `internal error` and nothing about disk. See the CI notes in `.github/workflows/rust.yml`.

## Stores that do not work

- **GCS.** `object_store` speaks the XML API; the emulators speak JSON. No emulator to test against.
- **Anything needing streaming `PUT`.** See WebDAV above.

## Backend removal

Removing a backend does not move its content. `file_segments` rows still name it, and those files stop being servable. The admin endpoint answers `409` for a backend that still holds content.
