---
name: macos-ui-debugging
description: Launch, drive, screenshot and debug the transmission-remote-gtk GTK UI on macOS against an isolated local transmission-daemon. Use when you need to run the app, click, right-click, double-click, drag or type in it, take screenshots of its windows and menus, read its RPC log and GTK warnings, use the GTK inspector, or render its windows offscreen while the screen is locked.
---

# macOS UI debugging for transmission-remote-gtk

Everything goes through `scripts/ui-test/uitest` (run it from the repo root;
with no arguments it lists its commands). It uses:

- `scripts/ui-test/trgui.swift`: a small Swift tool for real input (Quartz
  CGEvent), the window list, the accessibility tree and menu bar items. `uitest`
  compiles it with `swiftc` into `target/ui-test/bin/trgui` when the source
  changes. You don't need to install anything.
- `screencapture` for screenshots, and Homebrew's `transmission-daemon`,
  `transmission-create` and `transmission-remote` in `/opt/homebrew/bin`.
- `scripts/ui-test/example-scenario.sh`: a complete example test that ends
  with `PASS`. Copy its patterns.

## Start and stop

```sh
cargo build                       # or: cargo build --features debug-snapshot (see below)
scripts/ui-test/uitest check      # permissions and screen lock
scripts/ui-test/uitest start      # daemon + 2 test torrents + scratch config + app
scripts/ui-test/uitest status
scripts/ui-test/uitest app        # restart only the app (after a rebuild); the daemon keeps its state
scripts/ui-test/uitest stop       # always do this when you're done
```

- The scratch dir is `target/ui-test/env` (gitignored; override with
  `TRG_UITEST_DIR`). `start` wipes and recreates it. It contains `daemon/`,
  `daemon.log`, `app.log`, `shots/`, `snapshots/`, and `xdg/config/transmission-remote-gtk/config.json`.
- The app runs with `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME` and
  `XDG_STATE_HOME` set to the scratch dir, so the real
  `~/.config/transmission-remote-gtk` is never read or written. Never start the
  app without these variables, and never use the /Applications bundle for tests.
- The daemon listens on `127.0.0.1:19091` (`TRG_UITEST_PORT`), with no auth,
  DHT, LPD, PEX, µTP or port mapping. It runs with `-f` in the background
  because the daemonized mode exits silently.
- The torrents are trackerless. `single-file.bin` (id 1) is complete and
  seeding. `multi` (id 2) has `a.txt`, `b.bin`, `sub dir/c.dat` and
  `sub dir/d.dat`, and none of it is downloaded.
- To query or change the daemon, use `uitest remote -l`, `uitest remote -t 2 -if`
  (the files and whether they're wanted), `uitest remote -t 2 -i`, and so on.
- The scratch config sets a 1000x700 window, notebook-paned-pos 300, auto-connect
  and a 1 s update interval. `kill` (what `stop` does) doesn't save the app's
  config. Cmd+Q (`uitest key cmd+q`) closes the window properly and saves it.
- The user may have their own instance of the app open, for example from
  /Applications. `uitest` only ever targets the pid in `env/app.pid`, never
  the app's name. Don't kill other instances.

## Drive the app and take screenshots

```sh
U=scripts/ui-test/uitest
$U front                      # bring it to the front first: input goes to whatever is under the pointer
$U windows                    # id  pid  layer  x,y wxh (points)  scale  owner  title
$U shot main --win $($U win)  # just the main window -> env/shots/main.png
$U shot menu                  # all of the app's windows, including popovers/menus
$U click 600 150              # points relative to the main window's top left corner
$U rclick 600 150             # right click: context menu
$U dclick 600 100             # double click: torrent properties
$U click 731 515 --win 7752   # relative to another window (a dialog)
$U click 1462 1030 --px       # pixels of a screenshot of that window (divided by its scale)
$U drag 700 597 700 350       # e.g. move a paned separator
$U scroll -3 --at 600 200
$U type mul                   # text, and also the tree views' type-ahead
$U key escape  key cmd+i  key shift+f10  key down return
$U menu Torrent Properties    # press a native menu bar item through accessibility
$U wait-window "multi" 5      # wait until a window with this title appears
```

- Coordinates are window-relative **points**. The window's bounds include the
  native 28 point title bar, and `shot --win` (`screencapture -o -l`) matches
  them exactly. On a Retina display, screenshots are 2x, so divide pixels by
  `scale` (from `uitest windows`) or pass `--px`.
- When you look at a screenshot, downscale it to the window's width in
  points, for example `sips -Z 1000 shot.png --out small.png` for a window
  1000 points wide (`-Z` limits the longer side). That makes image
  coordinates equal click coordinates. It also prevents the image viewer from
  rescaling it by some unknown factor.
- Popovers, context menus and dialogs are separate windows. They show up in
  `uitest windows` with their own id and global position. `shot NAME` without
  `--win` captures the area that covers all of the app's windows (with the
  desktop behind any gaps).
- Verify effects on the daemon rather than only in pixels. For example,
  `example-scenario.sh` unticks `b.bin` and then checks
  `uitest remote -t 2 -if` for `No`.
- The UI is asynchronous. After an action that sends RPC, wait about 1.5 to
  2.5 s (one update interval plus a round trip) before you check the result.
- Clicking a Download check box in the Files tab toggles it straight away. Its
  parent shows the mixed state.

## Logs, warnings, inspector

- `env/app.log` has the app's stderr. With `TRG_CLIENT_DEBUG=1` (the default
  in `uitest`; pass `TRG_CLIENT_DEBUG=0` to turn it off), it also has every
  RPC request and response. `>` lines are requests and `<` lines are
  responses. They're JSON-RPC 2.0 with snake_case names, often batches
  (arrays): an action and the torrent_get that follows it go in one
  request. Look for a request like this:
  `grep '"method":"torrent_set"' target/ui-test/env/app.log`.
  torrent_get uses the table format (an array of keys, then arrays of
  values), so grep for values (`'"multi"'`) rather than `"name":"multi"`.
  One `409 Conflict` at startup is the normal session-id handshake; its
  `X-Transmission-Rpc-Version` header tells the server's RPC version.
- To find GTK warnings, use `grep -E 'WARNING|CRITICAL' target/ui-test/env/app.log`.
  `Gdk-WARNING ... gdk_frame_timings_presented() called on skipped frame`
  is harmless noise of the macOS backend.
- Run `G_MESSAGES_DEBUG=all scripts/ui-test/uitest app` for GLib debug messages.
- The GTK inspector works. Run `GTK_DEBUG=interactive scripts/ui-test/uitest app`.
  It opens as a second window (client-side decorations, 800x500); find it
  with `uitest windows`. The startup messages `Failed to open display (null)`
  and `Using default display for GtkInspector` are expected.
- `uitest log` follows app.log, and `uitest log daemon` follows the daemon's log.

## Offscreen snapshots (screen locked, or no screen access)

Build with the cargo feature `debug-snapshot` (it's not in default or meson
builds). `uitest` always passes `TRG_DEBUG_SNAPSHOT=<env>/snapshots` to the
app.

```sh
cargo build --features debug-snapshot && scripts/ui-test/uitest app
scripts/ui-test/uitest snap                          # render all windows and popovers
scripts/ui-test/uitest snap win.view-notebook        # activate actions first, then render
scripts/ui-test/uitest snap "win.connect 0" app.quit # with a GVariant parameter
```

`snap` prints the PNG paths: `<n>-<title>.png`, and `<n>-<title>-popover-<m>.png`
for each open popover. They're rendered with the window's own GSK renderer
at its scale factor, without the native title bar. The mechanism is in
`src/debug_snapshot.rs`. The app polls for `<dir>/request` twice a second,
activates each line of it on the main window, removes it, and then writes
the PNGs and finally `index.txt`. `TRG_DEBUG_SNAPSHOT_INTERVAL=<s>` also renders
periodically. The action names are in `src/menus.rs` and `src/main_window.rs`,
for example `win.properties`, `win.view-states`, `win.pause` and
`win.remote-prefs`.

Real input doesn't work while the screen is locked, so this mode only
covers what you can reach with actions.

## Pitfalls

- **Permissions** belong to the process that runs the shell (the terminal),
  not to trgui. Accessibility is needed for clicks, keys, `ax` and `menu`.
  Screen Recording is needed for `screencapture` and for window titles in
  `uitest windows`: without it the titles come back empty and screenshots
  show only the desktop. `uitest check` reports both and whether the screen
  is locked.
- **A locked screen** or sleeping display makes screenshots black, or shows
  only the lock screen, and input goes nowhere. `uitest check` says `LOCKED`
  in that case. Use `uitest snap` instead.
- **The user is at the machine.** Clicks move their pointer and `front`
  steals focus. Keep sequences short, and run `front` again before input if
  anything else may have become frontmost. If a click seems to do nothing,
  take a full-screen `screencapture -x` to see what's on top.
- **Multiple displays.** CG coordinates are global points, with the origin at
  the top left corner of the main display. Windows on other displays can
  have negative x or y, and each display has its own scale. `uitest` always
  works relative to the window and uses that window's display scale, so
  don't hardcode screen positions. This was only tested with the single
  built-in Retina display.
- **Keys use US key codes.** `type` sends the US-layout key code of each
  character plus its text. GTK key handlers, such as the tree views'
  type-ahead, use the key code, so text-only events (key code 0) would
  arrive as "a". With another active keyboard layout, the letters that are
  on different keys in that layout may come out wrong in key handlers.
  Text entries are fine either way.
- **The accessibility tree** only exposes the native parts: the title bar
  buttons and the global menu bar with all of the app's menus. GTK 4.24 on
  macOS doesn't export its widgets, so find widgets in screenshots, and use
  `uitest menu ...` for anything that's in the menu bar.
- **The window position** comes from macOS (cascading), not from the config.
  Always use window-relative coordinates.
- **Layout-dependent coordinates.** The coordinates in `example-scenario.sh`
  are for the scratch config's window size and pane positions. If the UI
  changes, take a screenshot and read the new coordinates from it.
- **Drag and drop from Finder** (dropping .torrent files on the window) isn't
  covered. `drag` only drags inside the screen with the mouse button held,
  which works for paned separators, column resizing and selections.
