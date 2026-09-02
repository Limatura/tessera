#!/usr/bin/env bash
# Measure the image this run built, compare it against the last build of main,
# and write the result as a table — to the run summary, to a file the comment
# job picks up, and to `$GITHUB_OUTPUT`. Enforces the budgets last, so a build
# that is over still publishes the numbers that say by how much.
#
# Why each number is taken the way it is:
#
#   binary  `stat` on the executable copied out of the container. The one figure
#           a Rust change moves directly, so it is the one worth attributing.
#   rootfs  `docker export | wc -c`, the flattened container filesystem. Unlike
#           `docker image inspect` it means the same thing whether the daemon
#           uses the classic or the containerd image store, which is why the
#           budget is enforced on this one and not on the other.
#   image   what this daemon reports for the tag. Kept because it is the number
#           a reader sees locally, labelled as such, and never enforced.
#
# What a `docker pull` actually transfers is deliberately absent: that size only
# exists once the layers are compressed and pushed, and this runs before any
# push. A number that is only right on publish would read as a regression on
# every pull request.

set -euo pipefail

IMAGE=${1:?usage: image-size.sh <image-tag>}

BINARY_PATH=${BINARY_PATH:-/usr/local/bin/tessera-server}
BINARY_BUDGET_MB=${BINARY_BUDGET_MB:-0}
ROOTFS_BUDGET_MB=${ROOTFS_BUDGET_MB:-0}

# The baseline is a file on the runner, not a cache action. This runner keeps
# its filesystem between runs — the same property the buildx layer cache under
# /tmp already relies on — and a file costs no upload, no restore key and no
# fourth-party action in a job that already has eight.
#
# If the machine is ever replaced the file is gone and the comparison column
# reads "—", which is the right failure: no baseline is better than one carried
# over from a different toolchain on a different machine.
STATE_DIR=${STATE_DIR:-/tmp/tessera-image-size}
BASELINE=${BASELINE:-$STATE_DIR/main.txt}
CURRENT=${CURRENT:-$STATE_DIR/current.txt}
REPORT=${REPORT:-$STATE_DIR/report.md}

mkdir -p "$STATE_DIR"

# ---------------------------------------------------------------- measurement

cid=$(docker create "$IMAGE")
trap 'docker rm -f "$cid" >/dev/null 2>&1 || true' EXIT
docker cp "$cid:$BINARY_PATH" "$STATE_DIR/binary"
rootfs=$(docker export "$cid" | wc -c)

binary=$(stat -c%s "$STATE_DIR/binary")
image=$(docker image inspect "$IMAGE" --format '{{.Size}}')
rev=${GITHUB_SHA:-unknown}

{
  echo "rev $rev"
  echo "binary $binary"
  echo "rootfs $rootfs"
  echo "image $image"
} > "$CURRENT"

# ------------------------------------------------------------------ baseline

# Plain `key value` lines rather than JSON, read with awk rather than sourced:
# nothing in this file is ever interpreted as shell, so a corrupt or truncated
# baseline is a missing column instead of a command.
baseline_field() {
  [ -f "$BASELINE" ] || return 0
  awk -v key="$1" '$1 == key { print $2; exit }' "$BASELINE"
}

base_binary=$(baseline_field binary)
base_rootfs=$(baseline_field rootfs)
base_image=$(baseline_field image)
base_rev=$(baseline_field rev)

mb() { awk -v b="${1:-}" 'BEGIN { if (b == "") print "—"; else printf "%.1f MB", b / 1000000 }'; }

delta() {
  awk -v c="$1" -v b="${2:-}" 'BEGIN {
    if (b == "") { print "—"; exit }
    d = c - b
    if (d == 0) { print "no change"; exit }
    sign = (d > 0) ? "+" : "−"
    a = (d > 0) ? d : -d
    if (b > 0) printf "%s%.2f MB (%s%.1f%%)", sign, a / 1000000, sign, a * 100 / b
    else printf "%s%.2f MB", sign, a / 1000000
  }'
}

budget() { awk -v m="$1" 'BEGIN { if (m + 0 == 0) print "—"; else printf "%d MB", m }'; }

# --------------------------------------------------------------------- report

{
  echo "### 🐳 \`$IMAGE\` 映像檔體積"
  echo
  echo "| 項目 | 本次 build | main 基準 | 變化 | 預算 |"
  echo "| --- | ---: | ---: | ---: | ---: |"
  echo "| 執行檔 binary | $(mb "$binary") | $(mb "$base_binary") | $(delta "$binary" "$base_binary") | $(budget "$BINARY_BUDGET_MB") |"
  echo "| rootfs(預算依據) | $(mb "$rootfs") | $(mb "$base_rootfs") | $(delta "$rootfs" "$base_rootfs") | $(budget "$ROOTFS_BUDGET_MB") |"
  echo "| image(daemon 回報) | $(mb "$image") | $(mb "$base_image") | $(delta "$image" "$base_image") | — |"
  echo
  if [ -n "$base_rev" ]; then
    echo "<sub>基準:main @ \`${base_rev:0:7}\` · rootfs 取自 \`docker export\`,預算檢查以它為準 · 壓縮後的 pull 體積要 push 完才存在,這裡不量</sub>"
  else
    echo "<sub>這台 runner 還沒有基準 — 下一次 main build 成功後就會寫入</sub>"
  fi
  echo
  echo "<details><summary>最大的幾層 layer</summary>"
  echo
  echo "| 大小 | 產生這層的指令 |"
  echo "| ---: | --- |"
  docker history --human=false --no-trunc --format '{{.Size}}	{{.CreatedBy}}' "$IMAGE" \
    | sort -t'	' -k1,1rn \
    | head -12 \
    | awk -F'	' '
        $1 + 0 == 0 { next }
        {
          cmd = $2
          sub(/^\/bin\/sh -c #\(nop\) +/, "", cmd)
          sub(/^\/bin\/sh -c +/, "", cmd)
          sub(/^RUN \/bin\/sh -c +/, "", cmd)
          gsub(/\|/, "\\|", cmd)
          # A backtick would close the code span this sits in; \140 and \47 keep
          # the substitution out of shell quoting.
          gsub(/\140/, "\47", cmd)
          if (length(cmd) > 80) cmd = substr(cmd, 1, 77) "..."
          printf "| %.1f MB | `%s` |\n", $1 / 1000000, cmd
        }'
  echo
  echo "</details>"
} > "$REPORT"

cat "$REPORT"

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  cat "$REPORT" >> "$GITHUB_STEP_SUMMARY"
fi

if [ -n "${GITHUB_OUTPUT:-}" ]; then
  {
    echo "binary_bytes=$binary"
    echo "rootfs_bytes=$rootfs"
    echo "image_bytes=$image"
    # A random delimiter is the documented way to stop a crafted body from
    # closing the block early. Nothing here is attacker-controlled today, but
    # the layer table is generated text and this costs one line.
    delim="TESSERA_REPORT_$(head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n')"
    echo "report<<$delim"
    cat "$REPORT"
    echo "$delim"
  } >> "$GITHUB_OUTPUT"
fi

# --------------------------------------------------------------------- budgets

status=0
binary_mb=$(( binary / 1000000 ))
rootfs_mb=$(( rootfs / 1000000 ))

if [ "$BINARY_BUDGET_MB" -gt 0 ] && [ "$binary_mb" -gt "$BINARY_BUDGET_MB" ]; then
  echo "::error::執行檔 ${binary_mb} MB,超過 ${BINARY_BUDGET_MB} MB 預算"
  status=1
fi
if [ "$ROOTFS_BUDGET_MB" -gt 0 ] && [ "$rootfs_mb" -gt "$ROOTFS_BUDGET_MB" ]; then
  echo "::error::映像檔 rootfs ${rootfs_mb} MB,超過 ${ROOTFS_BUDGET_MB} MB 預算"
  status=1
fi
exit $status
