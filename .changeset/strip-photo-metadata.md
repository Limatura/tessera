---
"tessera-server": patch
---

`tess send` removes the metadata from JPEG, PNG and WebP photos before they leave the machine: where they were taken, the camera, the time, comments. The orientation stays, so a photo still shows the right way up. `--keep-metadata` sends them as they are. Whether sending starts out removing it is the new `share.strip_metadata` setting, on by default and reported in `GET /api/v1/capabilities`. The web app will follow.
