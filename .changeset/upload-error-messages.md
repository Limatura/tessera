---
"tessera-server": patch
"web": patch
---

An instance that doesn't allow anonymous uploads now says so when you try to upload without signing in, instead of asking you to sign in again. A failed upload also says why when the request never got an answer, or when a proxy in front of the server refused it, instead of a generic "try again".
