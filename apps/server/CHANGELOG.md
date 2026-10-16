# tessera-server

## 0.1.0-rc.1

### Patch Changes

- [#4](https://github.com/Limatura/tessera/pull/4) [`224cb0a`](https://github.com/Limatura/tessera/commit/224cb0aae893298ae1847d8c7016978e1abfc605) Thanks [@sao-coding](https://github.com/sao-coding)! - The retrieval page (`/s`) now draws as many code boxes as `share.code_length` says, instead of a hard-coded 8; `GET /api/v1/capabilities` reports it as `code_length`. The code input asks phones for an ASCII keyboard, and the redundant Continue button is gone (a full code submits on its own). On the login page, Tab now goes username → password → "Forgot password?".

## 0.1.0-rc.0

### Minor Changes

- [`446a7a7`](https://github.com/Limatura/tessera/commit/446a7a757c03d9c80962aeccf5a2cf63dc22631d) Thanks [@sao-coding](https://github.com/sao-coding)! - First release candidate.
