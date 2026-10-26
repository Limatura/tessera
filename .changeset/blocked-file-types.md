---
"tessera-server": patch
---

Refuse file types by extension with `share.blocked_extensions`. Windows, Linux and macOS programs are also caught by their first bytes when renamed. Checked on upload and again on download. Files in end-to-end encrypted shares go through.
