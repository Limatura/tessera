# Malware scanning

Off until an operator points `scan.url` at a scanner. Tessera does not contain one: ClamAV's signatures alone are hundreds of megabytes and change daily, so built in they would be most of the image and always out of date. An operator who wants scanning runs a scanner and says where it is.

## Two kinds of scanner

- **`clamd://host:3310`**: ClamAV's daemon, spoken to directly with `INSTREAM`. The `clamav/clamav` image runs one.
- **`http://…` or `https://…`**: anything that takes the file as a `POST` body and answers `200` with `{"clean": true}` or `{"clean": false, "threat": "…"}`. `413` means the file is too large to scan. `scan.authorization`, when set, is sent as `Authorization`, and is stored sealed like the other credentials the panel takes.

## What happens to a file

It is recorded as `pending` the moment it is stored, and the scanner reads it back from the store in the background. Not inside the upload's request, for two reasons: a large file takes minutes to scan, and a file written straight to a bucket never passed through this process to be scanned on the way.

**Until a verdict comes back, the share is not handed out.** `/download`, `/raw`, `/segments` and `POST /shares/{code}/unlock` answer `423` with `Retry-After`, and the refusal comes before a retrieval is spent, so a one-time share is not used up by being turned away. The share's own description says `"scanning": true` in the meantime. Most files take well under a second.

| Verdict | What it means for the share |
| --- | --- |
| `clean` | Handed out as usual. |
| `infected` | Switched off, as an operator would switch it off: it answers `404`. The threat's name is kept in `scan_detail`. |
| `skipped` | Handed out unscanned. `scan_detail` says why: `encrypted`, or `too_large` when the scanner refused the size. |
| `deferred` | The scanner could not be reached. Handed out, and scanned once it is back. |

The admin share listing (`GET /api/v1/admin/shares`) carries each share's `scan_state` and `scan_detail`, and a `status` of `disabled` for one switched off this way, so it can be told apart from one that expired or ran out of downloads.

## When the scanner is down

Nothing waits on it. The file being sent is marked `deferred`, and so is everything else still `pending`, so every share is handed out as if scanning were off. Every pass, once every thirty seconds and whenever something is uploaded, the deferred files are tried again; a threat found then switches the share off late rather than never. Any answer that is not a verdict counts as down: a refused connection, a timeout, an HTTP `500`, a clamd error.

Turning `scan.url` off does the same for whatever was still pending: nothing is left to deliver a verdict.

## Encrypted shares

A scanner reads plaintext, and an end-to-end encrypted share is ciphertext from its first byte, so there is nothing to scan. Its files are `skipped` with `encrypted` and never held. Checking in the browser before sealing would not help: someone uploading malware on purpose controls their own browser.

So with both switched on, encryption is a way around scanning. That is left to the operator: an instance that must scan everything should not turn `features.encryption` on.

## Limits

- **clamd's `StreamMaxLength`**, 100 MiB in the `clamav/clamav` image. Larger files are `skipped` with `too_large`. Raise it in `clamd.conf`, up to 4 GiB, to scan larger ones.
- **Ten minutes** per file from connecting to the verdict, and ten seconds to connect, before the scanner counts as down.
- **Four files at a time**, so one large file does not keep every small one behind it held.

## Tested

`tests/scanning.rs`, against a clamd and an HTTP scanner that listen on real sockets. `a_real_clamd_agrees` runs the same checks against a real ClamAV (`TESSERA_TEST_CLAMD=clamd://127.0.0.1:3310`); it passed against ClamAV 1.5.4, including the EICAR test file and a file past the size limit.
