#!/bin/bash
# An example UI test with real input: select a torrent, open its context
# menu with a right click, untick and tick a file's Download check box in
# the Files tab and check the result on the daemon, then quit with Cmd+Q.
#
# The coordinates are points in the main window (including its 28 point
# title bar), for the 1000x700 window and the pane positions of the scratch
# config that uitest writes. If the layout changes, take a screenshot
# (uitest shot NAME --win $(uitest win)) and read them off it: pixels / 2
# on a Retina display.

set -euo pipefail
u=$(cd "$(dirname "$0")" && pwd)/uitest
dir=${TRG_UITEST_DIR:-$(cd "$(dirname "$0")/../.." && pwd)/target/ui-test/env}

wanted() { "$u" remote -t "$id" -if | awk -v f="$1" '$NF == f { print $4 }'; }
fail() { echo "FAIL: $*" >&2; "$u" stop; exit 1; }

"$u" stop >/dev/null 2>&1 || true
"$u" start
id=$("$u" remote -l | awk '$NF == "multi" { print $1 }')

# connected once the first torrent-get response with our torrents is logged
for _ in $(seq 50); do grep -q '"name":"multi"' "$dir/app.log" && break; sleep 0.2; done
grep -q '"name":"multi"' "$dir/app.log" || fail "the app didn't connect"
sleep 1

"$u" front
"$u" click 600 150 # the "multi" row
sleep 0.5
"$u" rclick 600 150
sleep 1
"$u" windows
"$u" shot context-menu
[[ $("$u" windows | wc -l) -ge 2 ]] || fail "no context menu"
"$u" key escape
sleep 0.5

"$u" click 192 390 # the Files tab
sleep 1.5
"$u" shot files-before --win "$("$u" win)"
[[ $(wanted multi/b.bin) == Yes ]] || fail "b.bin isn't wanted to start with"

"$u" click 731 515 # b.bin's Download check box
sleep 2.5
"$u" shot files-unticked --win "$("$u" win)"
[[ $(wanted multi/b.bin) == No ]] || fail "b.bin is still wanted after unticking it"
echo "OK: unticked b.bin, the daemon says it's not wanted"

"$u" click 731 515
sleep 2.5
[[ $(wanted multi/b.bin) == Yes ]] || fail "b.bin isn't wanted after ticking it again"
echo "OK: ticked b.bin again"

"$u" key cmd+q
for _ in $(seq 25); do kill -0 "$(cat "$dir/app.pid")" 2>/dev/null || break; sleep 0.2; done
kill -0 "$(cat "$dir/app.pid")" 2>/dev/null && fail "the app didn't quit on Cmd+Q"
echo "OK: quit; the scratch config now has $(grep -c '"' "$dir/xdg/config/transmission-remote-gtk/config.json") lines with keys"
"$u" stop
echo "PASS (screenshots in $dir/shots)"
