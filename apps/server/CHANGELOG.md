# tessera-server

## 0.1.0

### Minor Changes

- [`446a7a7`](https://github.com/Limatura/tessera/commit/446a7a757c03d9c80962aeccf5a2cf63dc22631d) Thanks [@sao-coding](https://github.com/sao-coding)! - First release candidate.

- [#73](https://github.com/Limatura/tessera/pull/73) [`4aa8f7c`](https://github.com/Limatura/tessera/commit/4aa8f7c4e9284f6ecef69578cea7b3520bdc77d6) Thanks [@timo9378](https://github.com/timo9378)! - Adds `tess`, a command-line client: `tess send` (files or a directory, optional end-to-end encryption, resumes an interrupted upload), `tess get`, `tess ls`, `tess rm`, and `tess login`, which signs in through the new `/device` page in the browser. Releases now carry `tess` and server binaries for Linux, macOS and Windows, with an install script. The server also accepts uploads from plain `curl -T` and serves `/llms.txt`.

### Patch Changes

- [#4](https://github.com/Limatura/tessera/pull/4) [`224cb0a`](https://github.com/Limatura/tessera/commit/224cb0aae893298ae1847d8c7016978e1abfc605) Thanks [@sao-coding](https://github.com/sao-coding)! - The retrieval page (`/s`) now draws as many code boxes as `share.code_length` says, instead of a hard-coded 8; `GET /api/v1/capabilities` reports it as `code_length`. The code input asks phones for an ASCII keyboard, and the redundant Continue button is gone (a full code submits on its own). On the login page, Tab now goes username → password → "Forgot password?".

## 0.1.0-rc.2

### Minor Changes

- [#73](https://github.com/Limatura/tessera/pull/73) [`4aa8f7c`](https://github.com/Limatura/tessera/commit/4aa8f7c4e9284f6ecef69578cea7b3520bdc77d6) Thanks [@timo9378](https://github.com/timo9378)! - Adds `tess`, a command-line client: `tess send` (files or a directory, optional end-to-end encryption, resumes an interrupted upload), `tess get`, `tess ls`, `tess rm`, and `tess login`, which signs in through the new `/device` page in the browser. Releases now carry `tess` and server binaries for Linux, macOS and Windows, with an install script. The server also accepts uploads from plain `curl -T` and serves `/llms.txt`.

## 0.1.0-rc.1

### Patch Changes

- [#4](https://github.com/Limatura/tessera/pull/4) [`224cb0a`](https://github.com/Limatura/tessera/commit/224cb0aae893298ae1847d8c7016978e1abfc605) Thanks [@sao-coding](https://github.com/sao-coding)! - The retrieval page (`/s`) now draws as many code boxes as `share.code_length` says, instead of a hard-coded 8; `GET /api/v1/capabilities` reports it as `code_length`. The code input asks phones for an ASCII keyboard, and the redundant Continue button is gone (a full code submits on its own). On the login page, Tab now goes username → password → "Forgot password?".

## 0.1.0-rc.0

### Minor Changes

- [`446a7a7`](https://github.com/Limatura/tessera/commit/446a7a757c03d9c80962aeccf5a2cf63dc22631d) Thanks [@sao-coding](https://github.com/sao-coding)! - First release candidate.
