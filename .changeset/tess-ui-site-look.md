---
"tessera-server": minor
---

`GET /api/v1/capabilities` reports the instance's look (`appearance`: mode, theme, accent), so a client that draws its own screen can match the site. The full-screen view takes its accent from there, after the account's own choice on the site and `[ui] accent` in the config file. Its send form follows the instance too: no encryption switch where encryption is off, no expiry longer than allowed, what the default expiry is, and a word to sign in first where uploads need an account.
