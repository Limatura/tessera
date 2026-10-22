---
"tessera-server": patch
---

`tess send --encrypt` seals about ten times faster, and the server hashes uploads about twice as fast. The release build optimised the crates doing this for size, which held encryption to about 100 MB/s.
