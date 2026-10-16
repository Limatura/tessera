#!/usr/bin/env bash
# Assemble the rows and detail blocks `bundle-size.sh` left behind (one call
# per app) into a single table, write it to the run summary and to
# `$GITHUB_OUTPUT` for the comment job. Separate from `bundle-size.sh` because
# that script runs once per app and this runs once, after all of them.

set -euo pipefail

STATE_DIR=${STATE_DIR:-/tmp/tessera-bundle-size}
REPORT=${REPORT:-$STATE_DIR/report.md}
REV=${GITHUB_SHA:-unknown}

{
  echo "### 📦 Frontend bundle size"
  echo
  echo "| app | Download (gzip) | main baseline | Change | Uncompressed |"
  echo "| --- | ---: | ---: | ---: | ---: |"
  [ -f "$REPORT.rows" ] && cat "$REPORT.rows"
  echo
  echo "<sub>Download: js/css/html/json/svg estimated with gzip -6; fonts, images and other already-compressed files counted at their raw size · The main baseline is the last successful build of main</sub>"
  [ -f "$REPORT.details" ] && cat "$REPORT.details"
} > "$REPORT"

cat "$REPORT"

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  cat "$REPORT" >> "$GITHUB_STEP_SUMMARY"
fi

if [ -n "${GITHUB_OUTPUT:-}" ]; then
  delim="TESSERA_REPORT_$(head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  {
    echo "report<<$delim"
    cat "$REPORT"
    echo "$delim"
  } >> "$GITHUB_OUTPUT"
fi

echo "rev $REV" > "$STATE_DIR/rev.txt"
