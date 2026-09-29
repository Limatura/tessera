#!/usr/bin/env bash
# Measure the image this run built, compare it against the last build of main,
# and write the result as a table — to the run summary, to a file the comment
# job picks up, and to `$GITHUB_OUTPUT`. Enforces the budgets last, so a build
# that is over still publishes the numbers that say by how much.
#
# The table answers two questions and nothing else, because those are the two a
# reader actually has:
#
#   下載大小  what `docker pull` transfers. Every layer gzipped, summed by
#             `image-compressed-size.py` — see that file for why it is not
#             `docker image inspect`.
#   磁碟大小  what the image occupies once unpacked: `docker export | wc -c`,
#             the flattened container filesystem. Store-independent, and the
#             figure the budget is enforced on.
#
# The binary is reported underneath the disk figure rather than beside it. It is
# not a third kind of size — it is the part of that number a Rust change moves,
# and on this image it is nearly all of it.
#
# Under the binary, when `FRONTEND_DIR` holds the two exported bundles: how much
# of it is the embedded web and admin SPAs, and the rest as the backend.
# rust-embed stores the files as they are, so the bundles' bytes on disk are
# what they add to the binary, give or take a few bytes of index per file. That
# makes the disk figure the frontend and backend together, counted once; adding
# the frontend bundle report to it would count the SPAs twice.
#
# `docker image inspect --format '{{.Size}}'` is deliberately absent. It was in
# this table once and reported 17.6 MB for an image holding a 34.6 MB binary.
# The cause was never established; what was established is that the field is not
# a stable quantity. Moby has rewritten it repeatedly (moby#49535, #49707,
# #52482, #52901), users have reported it doubling across an upgrade (#51579,
# #50103), and moby's own issue on the subject calls it "the size of the
# Manifest (and compressed layers???)" (#43862). Today the containerd path adds
# packed and unpacked content together. Whatever it means this month, it is not
# comparable with the two figures above, and not comparable with itself across
# a daemon upgrade — which is the whole job of the `main 基準` column.

set -euo pipefail

IMAGE=${1:?usage: image-size.sh <image-tag>}

BINARY_PATH=${BINARY_PATH:-/usr/local/bin/tessera-server}
FRONTEND_DIR=${FRONTEND_DIR:-}
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
# shellcheck disable=SC2329  # invoked by the trap below, which shellcheck does not follow
cleanup() {
  docker rm -f "$cid" > /dev/null 2>&1 || true
  # The saved archive is the size of the image again and this job runs against a
  # disk floor. It exists for one `python3` call and goes on the way out, on the
  # failure path as well as the success one.
  rm -f "$STATE_DIR/image.tar" "$STATE_DIR/binary"
}
trap cleanup EXIT

docker cp "$cid:$BINARY_PATH" "$STATE_DIR/binary"
rootfs=$(docker export "$cid" | wc -c)
binary=$(stat -c%s "$STATE_DIR/binary")

docker save "$IMAGE" -o "$STATE_DIR/image.tar"
compressed=$("$(dirname "$0")/image-compressed-size.py" "$STATE_DIR/image.tar")

frontend=""
backend=""
if [ -n "$FRONTEND_DIR" ] && [ -d "$FRONTEND_DIR" ]; then
  frontend=$(find "$FRONTEND_DIR" -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
  backend=$(( binary - frontend ))
fi

rev=${GITHUB_SHA:-unknown}

{
  echo "rev $rev"
  echo "binary $binary"
  echo "rootfs $rootfs"
  echo "compressed $compressed"
  if [ -n "$frontend" ]; then
    echo "frontend $frontend"
    echo "backend $backend"
  fi
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
base_compressed=$(baseline_field compressed)
base_frontend=$(baseline_field frontend)
base_backend=$(baseline_field backend)
base_rev=$(baseline_field rev)

mb() { awk -v b="${1:-}" 'BEGIN { if (b == "") print "—"; else printf "%.1f MB", b / 1000000 }'; }

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

budget() { awk -v m="$1" 'BEGIN { if (m + 0 == 0) print "—"; else printf "%d MB", m }'; }

# --------------------------------------------------------------------- report

{
  echo "### 🐳 \`$IMAGE\` 映像檔體積"
  echo
  echo "| 項目 | 本次 build | main 基準 | 變化 | 預算 |"
  echo "| --- | ---: | ---: | ---: | ---: |"
  echo "| **下載大小**(docker pull 傳輸量) | **$(mb "$compressed")** | $(mb "$base_compressed") | $(delta "$compressed" "$base_compressed") | — |"
  echo "| **磁碟大小**(pull 完解開後) | **$(mb "$rootfs")** | $(mb "$base_rootfs") | $(delta "$rootfs" "$base_rootfs") | $(budget "$ROOTFS_BUDGET_MB") |"
  echo "| &nbsp;&nbsp;└ 其中執行檔 | $(mb "$binary") | $(mb "$base_binary") | $(delta "$binary" "$base_binary") | $(budget "$BINARY_BUDGET_MB") |"
  if [ -n "$frontend" ]; then
    echo "| &nbsp;&nbsp;&nbsp;&nbsp;├ 後端程式 | $(mb "$backend") | $(mb "$base_backend") | $(delta "$backend" "$base_backend") | — |"
    echo "| &nbsp;&nbsp;&nbsp;&nbsp;└ 內嵌前端(web + admin) | $(mb "$frontend") | $(mb "$base_frontend") | $(delta "$frontend" "$base_frontend") | — |"
  fi
  echo
  if [ -n "$base_rev" ]; then
    echo "<sub>比較基準:main @ \`${base_rev:0:7}\` · 下載大小為各層 gzip 後加總,約等於 registry 實際存放的量 · 磁碟大小就是前後端加總,前端已經包在執行檔裡,不要再加上前端打包體積</sub>"
  else
    echo "<sub>這台 runner 還沒有基準 — 下一次 main build 成功後就會寫入</sub>"
  fi
  echo
  echo "<details><summary>各層 layer 明細(未壓縮)</summary>"
  echo
  echo "| 大小 | 產生這層的指令 |"
  echo "| ---: | --- |"
  # Sorted by size and cut off below 0.1 MB. A distroless base is a dozen bazel
  # targets of a few kilobytes each; listing them pushed the one layer that
  # matters — the binary — to the top of a screen of "0.0 MB" rows.
  docker history --human=false --no-trunc --format '{{.Size}}	{{.CreatedBy}}' "$IMAGE" \
    | sort -t'	' -k1,1rn \
    | awk -F'	' -v cut=100000 '
        BEGIN { shown = 0; hidden = 0; hidden_bytes = 0 }
        $1 + 0 == 0 { next }
        {
          if ($1 + 0 < cut || shown >= 10) { hidden++; hidden_bytes += $1; next }
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
          shown++
        }
        END {
          if (hidden > 0)
            printf "| %.1f MB | _另外 %d 層,每層皆小於 0.1 MB(distroless base)_ |\n",
                   hidden_bytes / 1000000, hidden
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
    echo "compressed_bytes=$compressed"
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
