---
"tessera-server": patch
"web": patch
---

An audit log: who signed in or failed to, who changed which setting, account, role, invite or API token, and what happened to shares (created, retrieved, downloaded, removed). Admins with the new `audit:read` permission (owner and admin roles) read it at `/admin/audit-log`, filtered by category, outcome, account, target and time, and can export it as CSV; the export is itself recorded. The users and all-shares pages link to the log filtered to one account or share. Entries are kept for `audit.retention_days` (180); share activity can be switched off with `audit.record_share_events`, sign-ins and admin changes cannot. Secrets, invite codes, typed sign-in names and wrong retrieval codes are never recorded, and an anonymous visitor's address is only kept while `share.access_log.enabled` is on.
