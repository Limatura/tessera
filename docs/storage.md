# Storage backends

Content past `share.inline_threshold_bytes` goes to an object store. Everything reaches it through `object_store`, so any backend that crate supports is a candidate. They do not all support the same operations, and the differences matter.

Code: `storage/mod.rs`, `storage/placement.rs`, `storage/first_write.rs`, `db/repository/storage_backend.rs`.

## What a backend is here

`storage.url` configures one backend, named `default`. More can be added at runtime through `POST /api/v1/admin/backends`; those live in the `storage_backends` table with their credentials sealed.

`storage::Pool` holds both: `open` maps a name to a live store, `configured` holds the rows.

Every `file_segments` row names the backend it lives on. A file stored before backends were configurable reads as `default`, because there was nowhere else it could have been.

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
