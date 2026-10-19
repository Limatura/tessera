#!/bin/sh
# Installs `tess`, the Tessera command-line client, from a GitHub Release.
#
#   curl -fsSL https://raw.githubusercontent.com/Limatura/tessera/main/scripts/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --version 0.1.0 --dir /usr/local/bin
#
# Without --version it installs the latest release that is not a pre-release.
# The archive is checked against the release's SHA256SUMS before anything is
# installed. Linux and macOS; on Windows, download tess-windows-x86_64.zip from
# the release page.
set -eu

repo=${TESS_REPO:-Limatura/tessera}
version=${TESS_VERSION:-}
dir=${TESS_INSTALL_DIR:-"$HOME/.local/bin"}

say() { printf '%s\n' "$*"; }
fail() {
	printf 'install.sh: %s\n' "$*" >&2
	exit 1
}

usage() {
	say "usage: install.sh [--version VERSION] [--dir DIRECTORY]"
	say ""
	say "  --version  a release such as 0.1.0; the latest stable one if left out"
	say "  --dir      where to put tess; ~/.local/bin if left out"
}

while [ $# -gt 0 ]; do
	case $1 in
	--version)
		[ $# -ge 2 ] || fail "--version needs a value"
		version=$2
		shift 2
		;;
	--dir)
		[ $# -ge 2 ] || fail "--dir needs a value"
		dir=$2
		shift 2
		;;
	-h | --help)
		usage
		exit 0
		;;
	*) fail "unknown option: $1" ;;
	esac
done

case $(uname -s) in
Linux) os=linux ;;
Darwin) os=macos ;;
*) fail "no build for $(uname -s); download tess from https://github.com/$repo/releases" ;;
esac

case $(uname -m) in
x86_64 | amd64) arch=x86_64 ;;
aarch64 | arm64) arch=aarch64 ;;
*) fail "no build for $(uname -m)" ;;
esac
# A shell running under Rosetta reports x86_64 on an Apple Silicon Mac, where
# the native build is the one to want.
if [ "$os" = macos ] && [ "$arch" = x86_64 ] &&
	[ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
	arch=aarch64
fi
archive="tess-$os-$arch.tar.gz"

if [ -n "${TESS_DOWNLOAD_BASE:-}" ]; then
	base=$TESS_DOWNLOAD_BASE
elif [ -n "$version" ]; then
	base="https://github.com/$repo/releases/download/v${version#v}"
else
	base="https://github.com/$repo/releases/latest/download"
fi

fetch() {
	if command -v curl >/dev/null 2>&1; then
		curl -fsSL -o "$2" "$1"
	elif command -v wget >/dev/null 2>&1; then
		wget -q -O "$2" "$1"
	else
		fail "needs curl or wget"
	fi
}

sha256() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$1" | cut -d ' ' -f 1
	elif command -v shasum >/dev/null 2>&1; then
		shasum -a 256 "$1" | cut -d ' ' -f 1
	else
		fail "needs sha256sum or shasum to check the download"
	fi
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

if ! fetch "$base/$archive" "$tmp/$archive"; then
	if [ -z "$version" ]; then
		fail "could not download $archive; if there is no stable release yet, pass --version"
	fi
	fail "could not download $archive from $base"
fi
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || fail "could not download SHA256SUMS from $base"

expected=$(grep " $archive\$" "$tmp/SHA256SUMS" | cut -d ' ' -f 1)
[ -n "$expected" ] || fail "SHA256SUMS has no line for $archive"
[ "$(sha256 "$tmp/$archive")" = "$expected" ] || fail "$archive does not match its checksum"

mkdir "$tmp/unpacked"
tar -xzf "$tmp/$archive" -C "$tmp/unpacked"
[ -f "$tmp/unpacked/tess" ] || fail "$archive has no tess in it"
mkdir -p "$dir"
cp "$tmp/unpacked/tess" "$dir/tess.tmp"
chmod 755 "$dir/tess.tmp"
# Renamed into place, so a tess already there is swapped whole, never half
# written over while it runs.
mv "$dir/tess.tmp" "$dir/tess"

say "Installed $("$dir/tess" --version) to $dir/tess"
case ":$PATH:" in
*":$dir:"*) ;;
*) say "$dir is not on your PATH. Add it with: export PATH=\"$dir:\$PATH\"" ;;
esac
say "Run tess to start, or tess --help for its commands."
