---
"tessera-server": patch
---

Webhooks. An account can be told when its shares are created, downloaded or expire (`/api/v1/webhooks`), and an administrator about every share (`/api/v1/admin/webhooks`). Bodies follow Standard Webhooks and are signed; `"format": "discord"` posts to a Discord channel instead. Failed events are retried for a day. An account's webhooks only go to public addresses, and only the instance's are told who downloaded something, while `share.access_log.enabled` is on. There is no page for them in the web app yet.
