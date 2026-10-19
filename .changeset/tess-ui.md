---
"tessera-server": minor
---

Adds `tess ui`, a full-screen view for picking what to send without typing paths: browse folders, filter by typing, find any file under a folder by a few letters of its path (skipping what `.gitignore` skips), mark files across folders, and send them. Pasting a share link downloads it into the folder you are in, and a second tab lists your shares to copy or delete. It works with the mouse, including selecting text to copy it over SSH, and with vim keys if you prefer; keys can be changed under `[ui]` in the config file. `tess send` with no files opens it too. The OpenTUI prototype in `apps/tui` is gone.
