#!/bin/bash
# The example UI test of example-scenario.sh, on Linux: in the headless
# GNOME Shell of the test container, with real input. Select a torrent, open
# its context menu with a right click, untick and tick a file's Download
# check box in the Files tab and check the result on the daemon, then quit
# with Ctrl+Q.
#
# Instead of coordinates read off a screenshot, it finds the widgets through
# the accessibility tree (uitest find), so it doesn't depend on the layout.
#
#   scripts/ui-test/uitest linux up && scripts/ui-test/uitest linux build
#   scripts/ui-test/example-scenario-linux.sh

set -euo pipefail
export TRG_UITEST_TARGET=linux
u=$(cd "$(dirname "$0")" && pwd)/uitest
dir=$(cd "$(dirname "$0")/../.." && pwd)/target/ui-test/env-linux

wanted() { "$u" remote -t "$id" -if | awk -v f="$1" '$NF == f { print $4 }'; }
fail() { echo "FAIL: $*" >&2; "$u" shot failure >&2 || true; "$u" stop; exit 1; }
# the center of a widget, in window coordinates: find ARGS... -> "x y"
at() { "$u" find "$@" | awk -F'\t' 'NR == 1 { print $1 }'; }

"$u" stop >/dev/null 2>&1 || true
"$u" start
id=$("$u" remote -l | awk '$NF == "multi" { print $1 }')

# connected once the first torrent_get response with our torrents is logged
for _ in $(seq 50); do grep -q '"multi"' "$dir/app.log" && break; sleep 0.2; done
grep -q '"multi"' "$dir/app.log" || fail "the app didn't connect"
sleep 1

"$u" front
# shellcheck disable=SC2046 # "x y"
"$u" click $(at --role label --name multi --exact) # the "multi" row
sleep 0.5
# shellcheck disable=SC2046
"$u" rclick $(at --role label --name multi --exact)
sleep 1
"$u" windows
"$u" shot context-menu
"$u" windows | grep -qE 'type=(popup|dropdown)-menu' || fail "no context menu"
"$u" find --role menu-item --name Properties --exact >/dev/null || fail "no Properties item in the context menu"
"$u" key escape
sleep 0.5

# shellcheck disable=SC2046
"$u" click $(at --role page-tab --name Files --exact)
sleep 1.5
"$u" shot files-before --win "$("$u" win)"
[[ $(wanted multi/b.bin) == Yes ]] || fail "b.bin isn't wanted to start with"

# b.bin's Download check box (--cell: GTK reports the widgets in column view
# cells at the cell's corner, but draws them centered)
# shellcheck disable=SC2046
"$u" click $(at --role check-box --inside b.bin --cell)
sleep 2.5
"$u" shot files-unticked --win "$("$u" win)"
[[ $(wanted multi/b.bin) == No ]] || fail "b.bin is still wanted after unticking it"
"$u" find --role check-box --inside b.bin | grep -qv checked || fail "the check box is still checked"
echo "OK: unticked b.bin, the daemon says it's not wanted"

# shellcheck disable=SC2046
"$u" click $(at --role check-box --inside b.bin --cell)
sleep 2.5
[[ $(wanted multi/b.bin) == Yes ]] || fail "b.bin isn't wanted after ticking it again"
echo "OK: ticked b.bin again"

"$u" key cmd+q # the primary modifier: Ctrl on Linux
# (the app's pid is a pid in the container: ask uitest)
for _ in $(seq 10); do "$u" status | grep -q 'app: *not running' && break; sleep 0.5; done
"$u" status | grep -q 'app: *not running' || fail "the app didn't quit on Ctrl+Q"
echo "OK: quit; the scratch config now has $(grep -c '"' "$dir/xdg/config/transmission-remote-gtk/config.json") lines with keys"
"$u" stop
echo "PASS (screenshots in $dir/shots)"
