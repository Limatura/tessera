#!/usr/bin/env bash
# Measure one app's `vite build` output and append it to the PR's bundle-size
# report. Called once per app (web, admin, ...) so each gets its own row and
# its own baseline; the workflow calls this once per app and then hands the
# accumulated $STATE_DIR/report.md to the comment step.
#
# Two figures, not one, because they answer different questions:
#
#   未壓縮  every byte under dist/ — what a checkout of the tag would occupy.
#   gzip    what a browser actually downloads. Summed per file rather than one
#           gzip of the tarball, and only over text assets (js/css/html/json/
#           svg): those are what a static-file server compresses on the way
#           out, while fonts and images arrive already compressed and gzipping
#           them again buys nothing but CPU — counting it as compressed would
#           understate what the browser really fetches for those files.
#
# Level 6 (gzip's own default) for the same reason as the server's image
# report: it is what most static-file serving compresses at, so this lands
# close to the real transfer without claiming to be exact.
#
# No budget here. The server image has one because its two numbers were fixed
# by a product decision this repo already made (see rust.yml); nobody has set
# a frontend bundle budget, so this script only reports — inventing a number
# to enforce would be a decision this script has no standing to make.

set -euo pipefail

LABEL=${1:?usage: bundle-size.sh <label> <dist-dir>}
DIST=${2:?usage: bundle-size.sh <label> <dist-dir>}

STATE_DIR=${STATE_DIR:-/tmp/tessera-bundle-size}
APP_DIR="$STATE_DIR/$LABEL"
BASELINE="$APP_DIR/main.txt"
CURRENT="$APP_DIR/current.txt"
REPORT=${REPORT:-$STATE_DIR/report.md}

mkdir -p "$APP_DIR"
[ -d "$DIST" ] || { echo "::error::${DIST} does not exist — did the build run first?"; exit 1; }

TEXT_EXT_RE='\.(js|mjs|css|html|json|svg|wasm)$'

# ------------------------------------------------------------------ measure

uncompressed=$(find "$DIST" -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')

gzip_total=0
while IFS= read -r -d '' f; do
  size=$(gzip -c -6 "$f" | wc -c)
  gzip_total=$(( gzip_total + size ))
done < <(find "$DIST" -type f -regextype posix-extended -iregex ".*${TEXT_EXT_RE}" -print0)

# Everything else (fonts, images, source maps) travels as-is.
other_bytes=$(find "$DIST" -type f -regextype posix-extended ! -iregex ".*${TEXT_EXT_RE}" -printf '%s\n' \
  | awk '{s+=$1} END{print s+0}')
transfer=$(( gzip_total + other_bytes ))

{
  echo "uncompressed $uncompressed"
  echo "transfer $transfer"
} > "$CURRENT"

# ------------------------------------------------------------------ baseline

baseline_field() {
  [ -f "$BASELINE" ] || return 0
  awk -v key="$1" '$1 == key { print $2; exit }' "$BASELINE"
}

base_transfer=$(baseline_field transfer)

mb() { awk -v b="${1:-}" 'BEGIN { if (b == "") print "—"; else printf "%.2f MB", b / 1000000 }'; }

delta() {
  awk -v c="$1" -v b="${2:-}" 'BEGIN {
    if (b == "") { print "—"; exit }
    d = c - b
    if (d == 0) { print "無變化"; exit }
    sign = (d > 0) ? "+" : "−"
    a = (d > 0) ? d : -d
    if (b > 0) printf "%s%.2f MB (%s%.1f%%)", sign, a / 1000000, sign, a * 100 / b
    else printf "%s%.2f MB", sign, a / 1000000
  }'
}

# ---------------------------------------------------------------- row + detail

{
  echo "| $LABEL | $(mb "$transfer") | $(mb "$base_transfer") | $(delta "$transfer" "$base_transfer") | $(mb "$uncompressed") |"
} >> "$REPORT.rows"

{
  echo
  echo "<details><summary>$LABEL 最大的檔案</summary>"
  echo
  echo "| 大小 | 檔案 |"
  echo "| ---: | --- |"
  find "$DIST" -type f -printf '%s\t%P\n' \
    | sort -t'	' -k1,1rn \
    | head -8 \
    | awk -F'	' '{ printf "| %.1f KB | `%s` |\n", $1 / 1000, $2 }'
  echo
  echo "</details>"
} >> "$REPORT.details"
