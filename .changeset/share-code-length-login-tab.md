---
"tessera-server": patch
"web": patch
---

The retrieval page (`/s`) now draws as many code boxes as `share.code_length` says, instead of a hard-coded 8; `GET /api/v1/capabilities` reports it as `code_length`. The code input asks phones for an ASCII keyboard, and the redundant Continue button is gone (a full code submits on its own). On the login page, Tab now goes username → password → "Forgot password?".
