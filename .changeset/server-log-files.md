---
"tessera-server": minor
---

The server keeps its log on disk as well as on stdout: one JSON-lines file per day under `data_dir/logs`, 3 days by default (`logs.retention_days`, `logs.min_level`). Accounts with the new `logs:read` permission, given to the owner and admin roles, can read, filter, stream and download it under `/api/v1/admin/logs`. Request lines now record the matched route (`/s/{code}`) instead of the raw path, so share codes stay out of the log.
