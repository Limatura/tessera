# tessera-server

## 0.3.0

### Minor Changes

- [#86](https://github.com/Limatura/tessera/pull/86) [`15c24b5`](https://github.com/Limatura/tessera/commit/15c24b59315fb6c885cc99a332ca71bb1a39ed32) Thanks [@timo9378](https://github.com/timo9378)! - The server keeps its log on disk as well as on stdout: one JSON-lines file per day under `data_dir/logs`, 3 days by default (`logs.retention_days`, `logs.min_level`). Accounts with the new `logs:read` permission, given to the owner and admin roles, can read, filter, stream and download it under `/api/v1/admin/logs`. Request lines now record the matched route (`/s/{code}`) instead of the raw path, so share codes stay out of the log.

- [#93](https://github.com/Limatura/tessera/pull/93) [`0030069`](https://github.com/Limatura/tessera/commit/0030069bc32fc0568c3f2602b5a72cc8ef9c5037) Thanks [@timo9378](https://github.com/timo9378)! - `GET /api/v1/capabilities` reports the instance's look (`appearance`: mode, theme, accent), so a client that draws its own screen can match the site. The full-screen view takes its accent from there, after the account's own choice on the site and `[ui] accent` in the config file. Its send form follows the instance too: no encryption switch where encryption is off, no expiry longer than allowed, what the default expiry is, and a word to sign in first where uploads need an account.

- [#89](https://github.com/Limatura/tessera/pull/89) [`afa15cc`](https://github.com/Limatura/tessera/commit/afa15cc408c1b550c2a0ec710b8c396bf35e57e8) Thanks [@timo9378](https://github.com/timo9378)! - Adds a full-screen view, opened by `tess` on its own, for picking what to send without typing paths: browse folders, filter by typing, find any file under a folder by a few letters of its path (skipping what `.gitignore` skips), mark files across folders, and send them. Pasting a share link downloads it into the folder you are in, and a second tab lists your shares to copy or delete. It works with the mouse, including selecting text to copy it over SSH, and with vim keys if you prefer; keys can be changed under `[ui]` in the config file. The OpenTUI prototype in `apps/tui` is gone.

### Patch Changes

- [#91](https://github.com/Limatura/tessera/pull/91) [`b59876f`](https://github.com/Limatura/tessera/commit/b59876fecd850959a5e92b12e4697b5a9d2d592b) Thanks [@timo9378](https://github.com/timo9378)! - `tess login` and `--server` take an instance's host alone (`share.example.com`), meaning HTTPS. In the full-screen view's send form, Enter and the arrow keys flip the encryption switch instead of sending, and the field the keys go to is marked.

- [#96](https://github.com/Limatura/tessera/pull/96) [`6a9b958`](https://github.com/Limatura/tessera/commit/6a9b958467ea0a02ed89c6a42e199741f9e29f1b) Thanks [@timo9378](https://github.com/timo9378)! - The full-screen view lists each share's expiry as a date and time on this machine's clock (`2026-10-26 09:56  7d`), not only as days left. `tess --help` and the README name `tess` on its own as the way into the full-screen view.

- [#100](https://github.com/Limatura/tessera/pull/100) [`cb89874`](https://github.com/Limatura/tessera/commit/cb898745ad4cbfc73c549c15895016066dc04d53) Thanks [@timo9378](https://github.com/timo9378)! - In the full-screen view, what is inside a marked folder shows as marked, and unmarking something there leaves it out of the share while the rest of the folder keeps its place. The send form counts a password that a key comes from up to the 12 characters it needs, and says so before sending rather than after. The QR codes are framed by a narrower margin, and `tess login` shows one even when it opens a browser.

- [#95](https://github.com/Limatura/tessera/pull/95) [`9cf7eb7`](https://github.com/Limatura/tessera/commit/9cf7eb7e039435addec8b6c699d538e227c9d9f2) Thanks [@timo9378](https://github.com/timo9378)! - `tess` on its own opens the full-screen view on a terminal, as a full-screen tool opens on its name; piped, it prints the help. The view's welcome, shown until there is an instance, carries the tilted ticket when the screen holds it, and the install script ends by saying to run `tess`. In it, Alt-← and Alt-→ go back and forward through the folders visited (`H` and `L` with the vim keys); Backspace still goes up one.

- [#101](https://github.com/Limatura/tessera/pull/101) [`463b63b`](https://github.com/Limatura/tessera/commit/463b63b638f5573ae4b69930ee300ac393782c4a) Thanks [@timo9378](https://github.com/timo9378)! - In the full-screen view, a QR code sits beside a share's details when the window is wide enough, under them when it is tall enough, and is left out with a word when neither, instead of being cut off. It is printed in the ticket's paper and ink, and with the lowest error correction, so it comes out smaller. Esc and Backspace close a share's details, which neither did, and the other dialogs. A share sealed with a password no longer says it opens only with its link.

- [#102](https://github.com/Limatura/tessera/pull/102) [`6577c80`](https://github.com/Limatura/tessera/commit/6577c80ed6cd08a781bfaf9819314857347f4f31) Thanks [@timo9378](https://github.com/timo9378)! - In the full-screen view, a QR code is drawn as a picture where the terminal can show one (kitty, Ghostty, WezTerm, iTerm2, Windows Terminal, foot and others with sixel), so it comes out square whatever the font; elsewhere it stays in half blocks. Its three corner squares take the accent, darkened where a light accent would not scan.

- [#103](https://github.com/Limatura/tessera/pull/103) [`0ef711d`](https://github.com/Limatura/tessera/commit/0ef711d4892380c43bad2d5302e2cb22bb81f639) Thanks [@timo9378](https://github.com/timo9378)! - The full-screen view's ticket moves: when something is sent, the dialog shows a small ticket whose stub tears off and falls away; while signing in or while a transfer runs, a small ticket crosses a short track in place of the spinner; and on the welcome screen, light passes over the large ticket every few seconds.

- [#92](https://github.com/Limatura/tessera/pull/92) [`4b9fdc5`](https://github.com/Limatura/tessera/commit/4b9fdc53788c026726bd7878dfc27b972317ec83) Thanks [@timo9378](https://github.com/timo9378)! - `tess` wears the site's ticket: `tess` on its own shows it with the commands to start with (and the install script ends with it), and the full-screen view is drawn in the site's orange, with the ticket on its welcome screen, in empty folders and lists, and atop the keys.

- [#98](https://github.com/Limatura/tessera/pull/98) [`d142a78`](https://github.com/Limatura/tessera/commit/d142a78a2b53eac7db03fd336ce7e47f9a5f426b) Thanks [@timo9378](https://github.com/timo9378)! - The full-screen view sends and downloads in the background: the view stays usable while they run, several can run at once, the footer shows how far the oldest has got, and F4 lists them all with their progress and a way to stop each. When one ends, its link (or what went wrong) shows as soon as the dialog in front is closed, and only the files it sent are unmarked.

- [#94](https://github.com/Limatura/tessera/pull/94) [`a9c6c43`](https://github.com/Limatura/tessera/commit/a9c6c434ddd15817f45aabdb945d9857a4de76fd) Thanks [@timo9378](https://github.com/timo9378)! - In the full-screen view, the buttons of a dialog take the arrow keys and Tab, and a question to delete or stop starts on the harmless answer. In the send form Ctrl-S sends from any field and the arrows go across Send and Cancel. A drag that leaves the window is still copied. F3 opens settings (keys, mouse, dotfiles, colour), saved to the config file as they change.

- [#99](https://github.com/Limatura/tessera/pull/99) [`aab7b6b`](https://github.com/Limatura/tessera/commit/aab7b6b358caed83e5c95f18626d2d37ff2ee1ad) Thanks [@timo9378](https://github.com/timo9378)! - The full-screen view makes the site's other two kinds of share: Ctrl-N (`n` with the vim keys) writes a note, with a title and a language for its highlighting taken from the title's extension, or makes a link that sends its visitors on. Enter on one of your shares shows it in full: kind, size, when it was made and expires, downloads so far and left, whether it is sealed or asks a password, its link and a QR code. Folders can be ordered newest or largest first (F6, `o` with the vim keys, or the label beside the path), saved with the other settings; the settings' Account row now does what it says. When the instance cannot be reached, the header says so and F5 tries again. `tess ls` names a link share `(link)` instead of `(note)`.

- [#97](https://github.com/Limatura/tessera/pull/97) [`1337da3`](https://github.com/Limatura/tessera/commit/1337da38c3c9f8dbd4d90d0f17fccce409e6ca32) Thanks [@timo9378](https://github.com/timo9378)! - The full-screen view signs in without leaving it: from the welcome screen, from a send that needs an account, from the Shares tab, or from the settings. It shows the code, the link and a QR code to approve on any device, waits, and saves the token as `tess login` does. With no instance yet, it asks for the address first.

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
