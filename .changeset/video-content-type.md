---
"tessera-server": patch
---

A share holding one video downloads with its video type (`video/mp4`, `video/webm`, `video/quicktime`, `video/x-matroska` or `video/ogg`, from the file's extension) instead of `application/octet-stream`, so Safari, iOS and Chromecast can play it. Encrypted shares and everything else stay `application/octet-stream`.
