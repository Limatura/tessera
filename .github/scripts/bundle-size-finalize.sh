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
  echo "### 📦 前端打包體積"
  echo
  echo "| app | 下載大小(gzip) | main 基準 | 變化 | 未壓縮 |"
  echo "| --- | ---: | ---: | ---: | ---: |"
  [ -f "$REPORT.rows" ] && cat "$REPORT.rows"
  echo
  echo "<sub>下載大小:js/css/html/json/svg 以 gzip -6 估算,字型與圖片等已壓縮資源以原始大小計入 · main 基準來自 main 分支上一次成功建置</sub>"
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
