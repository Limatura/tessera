---
"tessera-server": patch
---

The web app removes the metadata from JPEG, PNG and WebP photos before they are uploaded, the same as `tess send`, when the instance has `share.strip_metadata` on. For an end-to-end encrypted share it happens before sealing. A switch on the upload card keeps the metadata instead. The share settings in the admin panel have a switch for `share.strip_metadata`.
