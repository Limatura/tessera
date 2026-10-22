---
"tessera-server": patch
---

The server hashes passwords with argon2 0.6, the version the encryption code already used, so the binary no longer carries two copies. Stored passwords keep working.
