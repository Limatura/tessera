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

# A real download is a navigation to the attachment, not a page with a
# parameter: the page's script knows `plain`, `sw` and `opfs`, and answering
# `?case=download` does nothing at all. The first run spent ten minutes
# measuring Safari sitting on a page that downloaded nothing.
if [ "$kase" = "download" ]; then
  target="http://127.0.0.1:$port/attachment"
else
  target="http://127.0.0.1:$port/?case=$kase"
fi
# Safari from the previous case may still be quitting, and `open` against a
# browser that is on its way out does nothing at all. So: wait for it to be
# gone, start it, wait for it to be up, and only then navigate.
while pgrep -x Safari >/dev/null 2>&1; do sleep 1; done
open -a Safari
for _ in $(seq 1 30); do
  pgrep -x Safari >/dev/null 2>&1 && break
  sleep 1
done
sleep 2
open -a Safari "$target"

# Bounded by the clock, not by an iteration count: `footprint` costs a
# noticeable fraction of a second per process, so a loop counted in iterations
# runs for however long the sampler happens to take.
#
# The budget is what the transfer needs plus room to finish, and the loop ends
# the moment the server has its answer. It also gives up early if Safari never
# asks for anything, because a browser that started and loaded nothing looks
# exactly like one that ran, and reporting a floor as a measurement is the
# failure this guards.
# A flat ceiling rather than a figure derived from the delay, because the delay
# only bounds the rate from above: the same four gigabytes moved at 96 MiB/s on
# a Linux box and 15 MiB/s on this runner, so a budget computed from the ideal
# rate cut the slowest case short. The loop ends the moment the server has its
# answer, so a generous ceiling costs nothing on a case that works.
budget=900
started=$(date +%s)
peak=0
while kill -0 $server 2>/dev/null; do
  now=$(footprint_kib)
  [ "$now" -gt "$peak" ] && peak=$now
  elapsed=$(( $(date +%s) - started ))
  if [ "$elapsed" -gt 90 ] && ! grep -q "<- GET /" "$here/out-$port.log"; then
    echo "::error::Safari never requested anything on port $port"
    osascript -e 'tell application "Safari" to quit' >/dev/null 2>&1 || true
    kill $server 2>/dev/null
    exit 1
  fi
  [ "$elapsed" -gt "$budget" ] && break
  sleep 1
done

osascript -e 'tell application "Safari" to quit' >/dev/null 2>&1 || true
# Gone before the next case starts, for the reason above.
for _ in $(seq 1 20); do
  pgrep -x Safari >/dev/null 2>&1 || break
  sleep 1
done
pkill -x Safari 2>/dev/null || true
kill $server 2>/dev/null; wait $server 2>/dev/null
printf '%-9s peak=%6d KiB  ' "$kase" "$peak"
grep -E "^RESULT|report:" "$here/out-$port.log" | tail -1

# What the server saw, always. A case that produced no result is the one worth
# reading, and without this the only evidence is a peak that could mean
# anything.
echo "  --- what the server saw on $port ---"
grep -cE "<- GET /bytes|<- GET /attachment" "$here/out-$port.log" | sed 's/^/  byte requests: /'
tail -5 "$here/out-$port.log" | sed 's/^/  /'
if ! grep -qE "^RESULT \{\"case|report:" "$here/out-$port.log"; then
  moved=$(grep -c "  sent " "$here/out-$port.log" || true)
  if [ "${moved:-0}" -eq 0 ]; then
    echo "  stalled: the request was made and no bytes were consumed"
  else
    echo "  no completion: still moving after ${budget}s"
  fi
fi
