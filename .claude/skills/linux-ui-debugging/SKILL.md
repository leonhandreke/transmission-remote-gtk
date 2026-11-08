---
name: linux-ui-debugging
description: Launch, drive, screenshot and debug the transmission-remote-gtk GTK UI on Linux (GNOME Shell/Mutter, Wayland) in a Docker container, against an isolated local transmission-daemon. Use when you need to see or test the app on Linux, click, right-click, double-click, drag or type in it, find widgets through the accessibility tree (AT-SPI), take screenshots of its windows and menus, activate its actions over D-Bus, or read its RPC log and GTK warnings. Works from macOS and Linux hosts, and needs no screen.
---

# Linux UI debugging for transmission-remote-gtk

The Linux target of `scripts/ui-test/uitest`, the same tool as on macOS (see
the macos-ui-debugging skill), with the same commands. The app runs in a
container with a headless GNOME Shell. Run the commands from the repo root.

- **`uitest linux ...`** manages the container.
- **With `TRG_UITEST_TARGET=linux`**, every other `uitest` command runs inside
  the container. On a Linux host, that's the default.
- **Paths that `uitest` prints are host paths.** Inside the container, the repo
  is at `/src`.

## How it works

- **Image** (`scripts/ui-test/linux/Dockerfile`): `fedora:latest`, for the latest
  GNOME and GTK, plus Rust, the GTK build dependencies, `mutter-tests`,
  python-dbusmock, AT-SPI and `transmission-daemon`. The container mounts the
  repo at `/src`. Cargo's registry and target dir are in Docker volumes
  (`trg-uitest-cargo`, `trg-uitest-target`), so they're kept separate from
  the macOS `target/`.
- **Session** (`linux/session.sh`): GNOME Shell starts the way gnome-shell's
  own tests start it:
  - It runs under Mutter's D-Bus test runner (`linux/shell-dbus-runner.py`,
    with `gnomeshell_dbusrunner.py` and `dbusmock-templates/` copied from
    gnome-shell's tests). That gives it private session and system buses,
    with mocks for logind, AccountsService, GDM, polkit, UPower and so on.
  - The shell runs as `gnome-shell --headless --wayland --no-x11
    --virtual-monitor 1280x800 --automation-script linux/automation.js`, with a
    keyfile GSettings backend and animations off.
  - The automation script turns on the shell's **unsafe mode**, which allows
    `org.gnome.Shell.Eval`, `Introspect` and `Screenshot` over D-Bus.
  - The session also starts the accessibility bus.
  - It writes the bus addresses and `WAYLAND_DISPLAY` to
    `/run/user/1000/trg-session.env`. The helpers read that file.
- **Helper** (`linux/trgui.py`), the counterpart of `trgui.swift`:
  - Windows, focus and input go through `Eval` in the shell. The window list
    comes from Meta windows. Input comes from a Clutter virtual pointer and
    keyboard in the compositor, so it's real input with absolute coordinates.
  - Screenshots come from `org.gnome.Shell.Screenshot`.
  - The widget tree comes from AT-SPI.
  - Actions go through `org.gtk.Actions` on the app's bus name.
- **Renderer:** GTK renders with `GSK_RENDERER=cairo`, because the container
  has no GPU. The monitor scale is 1, so screenshot pixels equal click
  coordinates.

## Start and stop

```sh
U=scripts/ui-test/uitest
$U linux up                        # builds the image the first time (slow: Fedora mirrors), then about 20 s
$U linux build                     # cargo build in the container; --features debug-snapshot works too
export TRG_UITEST_TARGET=linux
$U start                           # daemon + 2 test torrents + scratch config + app, as on macOS
$U check                           # unsafe mode, overview hidden, AT-SPI ok
$U app                             # restart only the app (after linux build)
$U stop                            # stop the app and the daemon
$U linux restart                   # a fresh GNOME Shell session (after editing session.sh or automation.js)
$U linux down                      # remove the container when you're done
$U linux image                     # rebuild the image (newer Fedora, Dockerfile changes)
```

- **Scratch dir:** `target/ui-test/env-linux` (in the container,
  `/src/target/ui-test/env-linux`). It has the same layout and the same test
  torrents as on macOS: `single-file.bin` (id 1, complete) and `multi` (id 2:
  `a.txt`, `b.bin`, `sub dir/c.dat`, `sub dir/d.dat`, nothing downloaded).
- **The daemon** listens on `127.0.0.1:19091` inside the container. Use
  `uitest remote -l` and `uitest remote -t 2 -if` to query it.
- **Two copies of the source.** The app binary is the container's own build in
  `/home/tester/target/debug`. After changing the source, run
  `uitest linux build` before `uitest app`; a macOS `cargo build` doesn't
  affect it.
- **Inside the session.** `uitest linux shell` opens a shell there.
  `uitest linux shell CMD...` runs one command, for example
  `uitest linux shell gsettings list-recursively org.gnome.desktop.interface`.
- **Logs.** `uitest linux logs` shows the runner's and gnome-shell's logs.

## Drive the app and take screenshots

```sh
$U windows                    # id  pid  type=normal|dialog|dropdown-menu|...  x,y wxh  scale  app  "title"  [focus]
$U shot main --win $($U win)  # the main window's frame (no shadow) -> env-linux/shots/main.png
$U shot menu                  # the union of the app's windows, including open menus and popovers
$U find --role label --name multi --exact          # "x y<TAB>win=ID<TAB>role "name" extents states"
$U click $($U find --role page-tab --name Files | cut -f1)
$U rclick 600 150  dclick 600 150  drag 500 409 500 300  scroll 3 --at 600 200
$U type 'Mul X'               # any text; upper case and symbols work
$U key escape  key cmd+q  key ctrl+a backspace  key shift+f10
$U menu View "Torrent Details"                     # click the menu bar item, then press the item by label
$U action win.properties      # GActions over D-Bus: app.quit, "win.connect 0", win.view-states, ...
$U ax                         # the tree of showing widgets; ax --all for hidden ones too
$U wait-window "multi" 5      # wait for a normal or dialog window with this title
$U eval 'Main.overview.visible'                    # JavaScript in the shell
```

- **Coordinates.** They're logical pixels relative to the window's frame:
  its visible top left corner, with the header bar included and the
  client-side shadow excluded. AT-SPI extents (`ax`, `find`) and
  `shot --win` use the same coordinates, so you can pass `find`'s output, or
  a position read off a `shot --win` image, straight to `click`. `--px`
  divides by the scale, which is always 1 here.
- **`find`** prints the first showing match. Narrow the search with:
  - `--role` (roles as `ax` prints them: `button`, `page-tab`, `label`,
    `check-box`, `table-row`, `menu-item`, `text`, ...).
  - `--name`, a case-insensitive substring; `--exact` makes it exact.
  - `--inside TEXT`, an ancestor whose label contains TEXT, for example the
    Files tab row of a file: `find --role check-box --inside b.bin`.
  - `--all` prints every match.
- **Labels in `find` and `ax`.** For a widget with no name, they show the
  text of its `labelled-by` label; popover menu items work that way. Text
  entries also show their contents (`text="..."`), and check boxes show the
  `checked` state, so you can check UI state without screenshots.
- **`press`** does a widget's accessible action. Buttons and popover menu
  items have one (`click`). Menu bar items and check boxes in column views
  don't, so click those.
- **`menu` and `action` instead of a global menu bar.** Linux has none.
  `menu` clicks the menu bar item and then presses the menu items by label.
  `action` activates an action directly. The action names are in
  `src/menus.rs` and `src/main_window.rs`.
- **Keyboard shortcuts.** `cmd` in `key` means Ctrl, the primary modifier on
  Linux, so `key cmd+q` works on both platforms.
- **Check the daemon.** As on macOS, verify effects on the daemon, not only
  in pixels. After an action that sends RPC, wait about 1.5 to 2.5 s.
- **Example test:** `scripts/ui-test/example-scenario-linux.sh` runs the same
  scenario as the macOS `example-scenario.sh`, using `find` instead of
  coordinates, and ends with `PASS` in about 20 s. Copy its patterns.

## Logs, warnings, inspector, snapshots

- **App log.** `env-linux/app.log` has the app's stderr and, with
  `TRG_CLIENT_DEBUG=1` (the default), the RPC log, exactly as on macOS (see
  the macos-ui-debugging skill for how to read it). Use `uitest log` to
  follow it.
- **Warnings.** Find GTK warnings with
  `grep -E 'WARNING|CRITICAL' target/ui-test/env-linux/app.log`.
- **Environment variables.** `uitest` passes `GTK_DEBUG`, `G_MESSAGES_DEBUG`,
  `TRG_CLIENT_DEBUG`, `TRG_UITEST_PORT` and `TRG_DEBUG_SNAPSHOT_INTERVAL` on
  to the container. `GTK_DEBUG=interactive uitest app` opens the inspector as
  a second window: find it with `uitest windows`, take screenshots and click
  it like any other window.
- **Offscreen snapshots.** `uitest snap [ACTION...]` works as on macOS, after
  `uitest linux build --features debug-snapshot` and `uitest app`. The
  headless shell can always take screenshots, so you rarely need it here.

## Pitfalls

- **Clicks that do nothing** usually mean the overview (or another shell
  modal) is in the way: input goes to the shell then. `uitest check` says
  whether the overview is visible, and `uitest front` hides it. The session
  turns off hot corners, because the virtual pointer's first motion can hit
  the hot corner and open the overview.
- **Column view cells.** GTK 4.24 reports the widgets in column view cells
  (the Files tab's check boxes, for example) at the cell's top left corner,
  although it draws them centered. Clicking the reported center misses
  them. `find --cell` prints the center of the enclosing `table-cell`, which
  is right.
- **The tree walk is slow.** Every AT-SPI property is a D-Bus round trip.
  `find` and `ax` skip widgets that aren't showing (about 0.6 s for the main
  window). `ax --all` and `find --hidden` go into hidden notebook pages and
  rows and can take more than a minute.
- **The overview.** The shell shows it at startup, while there's no window.
  `uitest start`, `app` and `front` hide it. To see the whole screen, take
  `uitest linux shell python3 /src/scripts/ui-test/linux/trgui.py shot /src/target/ui-test/screen.png`.
- **Window types.** Windows and dialogs of the app are `type=normal`, and
  menus and popovers are `type=dropdown-menu`. The window position comes
  from Mutter, not from the config.
- **pids are pids in the container.** Don't `kill` or `kill -0` the pid in
  `env-linux/app.pid` from the host. Use `uitest status` and `uitest stop`.
- **Shell updates.** The image follows the latest Fedora. If a new
  gnome-shell breaks the mocked environment, compare
  `gnomeshell_dbusrunner.py` with gnome-shell's `tests/` and update it. If
  the shell doesn't get ready, `uitest linux logs` shows why.
- **No drag and drop from outside the app**, and no file chooser portal: the
  container has no xdg-desktop-portal. Dialogs that need a portal may fail.
