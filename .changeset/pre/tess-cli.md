---
"tessera-server": minor
"web": minor
---

Adds `tess`, a command-line client: `tess send` (files or a directory, optional end-to-end encryption, resumes an interrupted upload), `tess get`, `tess ls`, `tess rm`, and `tess login`, which signs in through the new `/device` page in the browser. Releases now carry `tess` and server binaries for Linux, macOS and Windows, with an install script. The server also accepts uploads from plain `curl -T` and serves `/llms.txt`.
