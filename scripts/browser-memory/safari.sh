#!/bin/bash
# One case, measured in Safari.
#
#   safari.sh <case> <size-bytes> <delay-ms> <port>
#
# Safari has no headless mode, so this needs a real login session. A GitHub
# macOS runner has one; a container does not.
#
# The metric is `footprint`'s phys_footprint, which is Apple's own accounting
# and the closest thing macOS has to the proportional set size used on Linux.
# It is not the same number, so a Safari figure is comparable with another
# Safari figure and not with a Linux one.
set -u
kase=$1; size=$2; delay=$3; port=$4
here=$(dirname "$0")

python3 "$here/server.py" "$size" "$delay" "$port" > "$here/out-$port.log" 2>&1 &
server=$!
sleep 1

footprint_kib() {
  # Every process WebKit splits itself across. Summed, because a download's
  # cost lands in the networking and content processes rather than in Safari.
  local total=0 pid
  for pid in $(pgrep -f "Safari|com.apple.WebKit" 2>/dev/null); do
    local kib
    kib=$(footprint -p "$pid" 2>/dev/null | awk '/phys_footprint/ {gsub(/[^0-9.]/,"",$(NF-1)); print int($(NF-1)); exit}')
    total=$((total + ${kib:-0}))
  done
  echo "$total"
}

open -a Safari "http://127.0.0.1:$port/?case=$kase"

peak=0
for _ in $(seq 1 2400); do
  kill -0 $server 2>/dev/null || break
  now=$(footprint_kib)
  [ "$now" -gt "$peak" ] && peak=$now
  sleep 0.25
done

osascript -e 'tell application "Safari" to quit' >/dev/null 2>&1 || true
kill $server 2>/dev/null; wait $server 2>/dev/null
printf '%-9s peak=%6d KiB  ' "$kase" "$peak"
grep -E "^RESULT|report:" "$here/out-$port.log" | tail -1
