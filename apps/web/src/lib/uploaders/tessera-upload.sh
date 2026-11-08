#!/bin/sh
# Uploads a file to Tessera and copies the link to the clipboard.
#
#   tessera-upload report.pdf
#   flameshot gui --raw | tessera-upload --name shot.png
#   echo hello | tessera-upload            (no name: becomes a note)
#
# This file contains an API key. Keep it private (chmod 700).
# TESSERA_URL and TESSERA_TOKEN in the environment override the values below.

set -eu

TESSERA_URL=${TESSERA_URL:-'__TESSERA_URL__'}
TESSERA_TOKEN=${TESSERA_TOKEN:-'__TESSERA_TOKEN__'}

usage() {
	echo "usage: tessera-upload [--name NAME] [FILE | -]" >&2
	exit 2
}

name=
file=-
while [ $# -gt 0 ]; do
	case $1 in
	-n | --name)
		[ $# -ge 2 ] || usage
		name=$2
		shift 2
		;;
	-h | --help) usage ;;
	--)
		shift
		break
		;;
	-?*) usage ;;
	*) break ;;
	esac
done
if [ $# -gt 1 ]; then
	usage
elif [ $# -eq 1 ]; then
	file=$1
fi
if [ "$file" != - ] && [ ! -f "$file" ]; then
	echo "tessera-upload: $file: no such file" >&2
	exit 1
fi
if [ -z "$name" ] && [ "$file" != - ]; then
	name=$(basename -- "$file")
fi

# Percent-encodes everything but unreserved characters, so a name with spaces
# or non-ASCII letters survives the trip in the path.
urlencode() {
	printf '%s' "$1" | od -An -v -tx1 | tr -d ' \n' | fold -w 2 |
		# `fold` leaves the last pair without a newline, which `read` reports as
		# a failure even though it filled `hex`.
		while read -r hex || [ -n "$hex" ]; do
			case $hex in
			2d | 2e | 5f | 7e | 3[0-9] | [46][1-9a-f] | [57][0-9a])
				printf "\\$(printf '%03o' "$((0x$hex))")"
				;;
			*) printf '%%%s' "$(echo "$hex" | tr 'a-f' 'A-F')" ;;
			esac
		done
}

# No name sends `PUT /-`, which the server turns into a note.
target=-
[ -z "$name" ] || target=$(urlencode "$name")

body=$(mktemp)
trap 'rm -f "$body"' EXIT INT TERM

status=$(curl -sS -o "$body" -w '%{http_code}' \
	-H "Authorization: Bearer $TESSERA_TOKEN" \
	-T "$file" "${TESSERA_URL%/}/$target") || exit 1

if [ "$status" != 201 ]; then
	# Errors come back as one `error: ...` line.
	cat "$body" >&2
	exit 1
fi

link=$(head -n 1 "$body")
printf '%s\n' "$link"

copy() {
	if [ -n "${WAYLAND_DISPLAY:-}" ] && command -v wl-copy >/dev/null; then
		wl-copy
	elif command -v xclip >/dev/null; then
		xclip -selection clipboard
	elif command -v xsel >/dev/null; then
		xsel --clipboard --input
	elif command -v pbcopy >/dev/null; then
		pbcopy
	elif command -v clip.exe >/dev/null; then
		clip.exe
	else
		return 1
	fi
}
printf '%s' "$link" | copy 2>/dev/null ||
	echo "tessera-upload: no clipboard tool found, link printed only" >&2
