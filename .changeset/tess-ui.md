---
"tessera-server": minor
---

Adds a full-screen view, opened by `tess` on its own, for picking what to send without typing paths: browse folders, filter by typing, find any file under a folder by a few letters of its path (skipping what `.gitignore` skips), mark files across folders, and send them. Pasting a share link downloads it into the folder you are in, and a second tab lists your shares to copy or delete. It works with the mouse, including selecting text to copy it over SSH, and with vim keys if you prefer; keys can be changed under `[ui]` in the config file. The OpenTUI prototype in `apps/tui` is gone.
