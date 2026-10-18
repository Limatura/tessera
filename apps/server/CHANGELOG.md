# tessera-server

## 0.2.0

### Patch Changes

- [#81](https://github.com/Limatura/tessera/pull/81) [`1d70d03`](https://github.com/Limatura/tessera/commit/1d70d03f51838b10137445d94837b7907db3d280) Thanks [@sao-coding](https://github.com/sao-coding)! - An instance that doesn't allow anonymous uploads now says so when you try to upload without signing in, instead of asking you to sign in again. A failed upload also says why when the request never got an answer, or when a proxy in front of the server refused it, instead of a generic "try again".

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
