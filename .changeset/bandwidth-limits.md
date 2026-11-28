---
"tessera-server": minor
"web": minor
---

Bandwidth limits. `bandwidth.upload` / `bandwidth.download` cap every transfer in that direction together, and `bandwidth.upload_per_transfer` / `bandwidth.download_per_transfer` cap each one on its own, in bytes per second (`0`, the default, is unlimited). They cover share uploads (direct, `curl -T` and resumable), share downloads and the speed test, and a change applies to transfers already running. Single-file downloads that are redirected to a signed object-store URL go straight from the store and are not limited. A per-transfer cap is per connection. The new Transfer speed settings page edits them in Mbit/s and suggests half of the last speed test result from this browser as a starting value.
