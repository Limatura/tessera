# winget

`Limatura.Tess` installs `tess` from `tess-windows-x86_64.zip` on the GitHub Release (a zip holding one portable `tess.exe`, exposed as the `tess` command).

The three manifests here are the first version, for submitting by hand once. After that `release.yml` opens a pull request on [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) for every stable release; it reads this package's last manifest there and updates the version, URL and hash, so nothing in this folder needs to follow along.

## One-time setup

1. Check the manifests: `winget validate contrib/winget` and `winget install --manifest contrib/winget` on a Windows machine (`winget settings --enable LocalManifestFiles` first).
2. Copy the three files to `manifests/l/Limatura/Tess/0.1.0/` in a fork of winget-pkgs and open a pull request. Moderators review it; it can take days.
3. Once it is merged, create a classic personal access token with `public_repo` on an account that has forked winget-pkgs, and save it as the `WINGET_TOKEN` repository secret. Without the secret the `winget` job in `release.yml` does nothing.
4. Add `winget install Limatura.Tess` to the README and the release notes.
