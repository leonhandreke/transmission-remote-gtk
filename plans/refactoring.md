# Refactoring plan: reducing the port's technical debt

The application was ported from C (GTK 3) to Rust (GTK 4), largely by an
agent, and still carries the shape of the C code: a GtkListStore-like
column model, JSON looked up by key everywhere, bit flags in plain
integers, and one large window that did everything. This plan lists what
is left to do, in the recommended order. Work happens on the
`refactor/tech-debt` branch.

## How to work

- Small, confident steps, one commit each. Every commit must build with no
  warnings: `cargo build`, `cargo clippy --all-targets` and `cargo test`
  clean, and `cargo fmt` applied. Also check
  `cargo build --features debug-snapshot` when touching the window or app.
- Commit messages: a short imperative subject, then a paragraph on why,
  ending with the `Co-Authored-By:` line of the model doing the work.
- Keep `po/POTFILES.in` in sync when adding, moving or removing source
  files that contain translatable strings (`tr(`, `ntr(`, `ctr(`).
- **Finding dead code:** rustc doesn't report unused `pub` items. On a
  clean tree, temporarily make everything `pub(crate)` and build:
  `perl -pi -e 's/^(\s*)pub (fn|const|struct|enum|type|static|use) /$1pub(crate) $2 /' $(git ls-files 'src/*.rs' 'src/**/*.rs') && cargo build`.
  Read the warnings, then `git checkout -- src` to revert. Never run that
  checkout with uncommitted work.
- **UI verification:** the `macos-ui-debugging` (and `linux-ui-debugging`)
  skills launch the app against an isolated transmission-daemon and drive
  it. Clicking through the UI is delegated to a subagent (Sonnet, in its own
  worktree); code review is not delegated. Remove the worktree afterwards.
- **Wire checks:** when typing a protocol response, compare the struct's
  field types with what a real `transmission-daemon` (4.1+) returns: start
  one in a scratch directory, add a torrent made with `transmission-create`
  (with a tracker URL), POST the JSON-RPC request (handle the 409 session
  ID handshake) and check each field's JSON type against the struct. This
  was done for `Torrent`; see the commit "Read the torrents of torrent_get
  into typed structs".
- The user decided against adding tests for now (step 1 below). Don't add
  test suites unless asked.

## Where things are (after the refactoring so far)

- `src/client/` — `mod.rs`: the client (preferences, transport, session,
  torrent model, request dispatch); `connection.rs`: connecting, the update
  timers, failure handling, `dispatch_interactive()` and friends, and the
  signals the window follows (`connection-changed`, `torrents-updated`,
  `request-failed`, `status-message`).
- `src/rpc/` — JSON-RPC 2.0 over libsoup. `src/protocol/` — method names,
  request keys, builders and small results (`mod.rs`), the typed torrent
  (`torrent.rs`) and session (`session.rs`).
- `src/json.rs` — the lenient `get_i64()`-style getters, for the preferences
  file and `JsonWidgets`.
- The list models hold GObjects with properties: `TorrentObject`
  (`torrent_object.rs`, in the `TorrentModel` of `torrent_model.rs`),
  `StateRow` (`state_selector.rs`), `PeerObject` (`peers.rs`),
  `TrackerObject` (`trackers.rs`), `FileObject` (`files.rs`) and
  `PersistentRow` (the preferences' lists, `dialogs/preferences.rs`).
- `src/tree_view.rs` — a GtkTreeView-like widget on GtkColumnView; its
  columns show a property of the row objects.
- `src/util.rs` — also the list helpers for any object type:
  `objects::<T>(model)`, `remove_where::<T>(store, stale)`, and
  `set_changed(obj, property, value)`.
- `src/main_window.rs` — the window: layout, wiring, filter, actions.
- `src/details.rs` — the trackers/files/peers lists of a torrent, shared by
  the window and the properties dialog.
- `src/dialogs/` — dialogs; `dialog.rs` is a GtkDialog-like base class.
- `src/ui/*.blp` — Blueprint UI files; `data/style.css` — the CSS.

## Steps

### 1. Tests for the model logic — skipped for now (user decision)

`TorrentModel::update` (first/active/full modes, stale removal), the files
tree in `FilesModel` (building, directories combining wanted/priority,
renames), `AddOptions::request` and `shorten_download_dir` have no tests.
They're nearly pure functions over data and would protect steps 3 and 4.

### 2. Typed protocol data instead of JSON lookups — done

`Torrent` (with `PeersFrom`, `Peer`, `File`, `TrackerStat`) in
`src/protocol/torrent.rs` and `Session` in `src/protocol/session.rs`; the
fields of torrent_get and session_get are derived from the structs
(`torrent_fields()`, `session_fields()`). The small results are
`SessionStats`, `PortTest`, `BlocklistUpdate`, `AddedTorrent` and a private
`FreeSpace`, parsed with `protocol::parse()`. Each was wire-checked against
Transmission 4.1.4. What's left in `src/json.rs` serves the preferences file
(profiles, destinations, custom headers) and `JsonWidgets`, which binds
dialog widgets to request keys (loaded from `Torrent::to_obj()` and
`Session::to_obj()`).

### 3. Typed list items instead of the column store — done

`Item` (`src/item.rs`) copied GTK 3's GtkListStore: a `Vec<Val>` of any
types, `COL_*` index constants (45 for a torrent), `item.int64(COL_X)`
reads, a `column-changed::cN` signal per column, and `TreeView` columns
registered by model column index. It's gone; every list model holds
GObjects with properties (see "Where things are"), and `TreeView` columns
show a property.

How `TreeView` works now (`src/tree_view.rs`):

- `reg_property(ctype, key, property, header, id, flags)` registers a
  column showing a property of the row objects; `set_extra()` (the icon
  name, the file ID) and `set_search_column()` take property names too.
  The cells read the property (as a private `Val`: strings, i64, i32, u32,
  f64, bools) and watch `notify::<name>`.
- Each column has a numeric `key`, which is what the sort column is saved
  as (`KEY_TV_SORT_COL`) and the target of the sort menu's actions. It's the
  model column index the column had as Items (written as numbers at the
  `reg_property()` calls), so saved sort settings keep their meaning. A
  saved key that isn't registered doesn't sort (with Items it sorted by
  that model column, but no menu offered such a key).
- Sorting compares the two rows' property values, unless the column has a
  typed compare function (`ColumnDesc::set_compare::<T>()`), which the
  torrent columns have (see below). `SortKey` compares i64, u32, f64 and
  strings (by collation) like the cells' values.
- Rows are `glib::Object`s in the API: `selected_items::<T>()`,
  `first_selected::<T>()`, `select_item()`, `item_at()`, `start_editing()`;
  `set_activate()` and `set_edit()` take closures over `&T`.
  `tree_view::row_object()` unwraps a `GtkTreeListRow`. The cell to start
  editing once it's shown is kept in the view (`imp::TreeView::start_edit`).

Setting a GObject property notifies even if the value doesn't change
(`Item::set` didn't), so the models set values with `util::set_changed()`,
which compares first; otherwise every update would refresh every bound
cell. Bookkeeping properties nobody shows (`serial`) use the plain setters.

**The torrents** (`src/torrent_object.rs`): a `TorrentObject` holds the
`Rc<Torrent>` of its last update and a `TorrentValues`, a plain struct of
the values the views show (flags, status text, icon, percent done, ratio,
seeders/leechers/downloads, peer sources, first tracker host, download
directory and its short form, and the torrent fields the columns show).
The `torrent_values!` macro defines the struct and makes each field a
read-only property, so a value is listed once. `set_values()` notifies the
properties that changed; the update sets the `serial` property last, which
always notifies, and which `TorrentRow` (the Transmission styles) watches.
Code that goes through all the torrents reads `obj.values()` and
`obj.torrent()` directly, not the properties: the filter
(`MainWindow::visible_func`), sorting, the stats, the sidebar's counting.
`Client::find_torrent(id)` replaced `torrent_data()`.

Why typed sorting (measured in a throwaway test, release build, 5000
objects, a full sort of about 65,000 comparisons): reading two properties
per comparison took 17 ms for an i64 column and 39 ms for a string column,
against 0.3 ms and 12 ms with Items; the typed compare functions take
0.8 ms for an i64 column and 23 ms for a string one (the collation,
`g_utf8_collate`, dominates; it did with Items too). Debug builds are 10
to 20 times slower. The window resorts after every update, but the list
is then mostly sorted, which GTK's sort (a timsort) handles in about n
comparisons.
`set_values()` for 5000 torrents takes about 3 ms (release). If sorting by
name is ever too slow, compare collation keys (`g_utf8_collate_key`)
cached in the values.

### 4. Split `tree_view.rs` (1,800 lines) — next

Separate the cell widgets (factories, editable labels, the wanted check
box, the priority cell, with `Val` and its formatting), the persistence
of column layout and sorting to the preferences, and type-to-search. The
type-to-search is duplicated almost verbatim in `state_selector.rs`
(`key_pressed`), as is presenting popovers in `size_allocate` (both
widgets' `WidgetImpl`): share them.

### 5. The torrent filter as an enum

`state_selector.rs` packs its tracker and directory filters into the same
`u32` as the torrent status flags (`FILTER_FLAG_TRACKER = 1 << 14`,
`FILTER_FLAG_DIR = 1 << 15`), and the window decodes that plus a "selected
text" (`MainWindow::refilter`, `visible_func`, fields `filter_criteria`,
`filter_selected_text`). Use
`enum Filter { All, State(u32), Tracker(String), Directory(String) }`,
returned by the selector. Optionally turn `TORRENT_FLAG_*`
(`protocol/torrent.rs`) into a `bitflags` type.

### 6. The preferences as the single source of truth for view settings

The view toggles (sidebar, directory filters, tracker filters, directories
first, torrent details) take two routes: the preferences dialog's
check boxes call the state selector and the window directly
(`dialogs/preferences.rs`: `view_states_toggled_cb` and friends, through
`win.state_selector()` and `win.notebook_set_visible()`), while the menu's
stateful actions write the preference and the window follows the actions'
state (`MainWindow::connect_view_actions`, `menu_bar.rs::view_item_new`).
Make the widgets follow preference changes only (`Prefs::connect_changed`),
so the preferences dialog no longer needs the window.

### 7. A `TorrentList` widget

`MainWindow` (about 1,080 lines) is mostly wiring now. The last cohesive
chunk in it is the torrent list: the tree view, its `CustomFilter` and
`visible_func`, the search entry handling (`filter_*_cb`, `key_pressed`),
the "No Results"/"Not Connected" overlay (`update_no_results`,
`update_connection`'s stack) and the context menu. Make it a composite
widget with a small API (selected IDs, refilter, resort). Saving the window
size and pane positions, and the torrent added/completed notifications,
can stay in the window.

### Smaller items

- **CI** — the workflow is `.github/workflows/c-cpp.yml`; rename it, and add
  `cargo test` and `cargo clippy --all-targets -- -D warnings`.
- **The `Dialog` base class** (`dialogs/dialog.rs`) reproduces GtkDialog's
  response IDs. Most dialogs only need OK/Cancel buttons calling closures;
  consider plain `gtk::Window`s with buttons.
- **Typed errors** — `Client::parse_settings` and adding torrents return
  `String` errors; small error enums would do.
- **gettext** — `i18n.rs` calls gettext through hand-written FFI;
  `gettext-rs` could replace it. Its C-style `sprintf` has to stay for the
  existing translations.
- **The About dialog** doesn't close with Escape on macOS (seen in UI
  testing on the base commit too).

## UI verification

Everything up to commit a17c365 was clicked through on macOS against
Transmission 4.1.4, and passed: connecting, disconnecting, failures while
connected and while connecting, periodic updates, the sidebar's tracker
filter, the details pages (trackers added, edited and deleted; files'
check boxes, priorities, context menu, rename, sorting and type-ahead),
column menus and their persistence across restarts, the torrent context
menu's limits and priority, the properties, remote preferences and
statistics dialogs, adding (with "Apply to all") and removing torrents, the
torrent rows' status and the First Tracker column.

Not covered: the peers list with real peers (type-ahead, host lookups).

After the torrents and the preferences' lists moved off `Item` (commits
2e1022d, 6362aaa, ff4eb7c), not clicked through yet; check:

- the torrent list in the classic style: every column's values and
  formats (sizes, speeds, ETA, ratio, dates, priority, queue position,
  the hidden peer columns from the column menus), the Name column's status
  icon, sorting by every column from the headers and the View > Sort menu,
  ascending and descending, and that values updating re-sort the list;
- a saved sort, column order and widths of the torrent list (from a build
  before these commits too) surviving a restart;
- the Transmission and compact styles: the status and progress texts for
  downloading, seeding (with and without a ratio goal), paused, queued,
  checking, errors (red) and metadata downloading; paused and queued rows
  dimmed; the progress bars;
- the sidebar: the state counts, the tracker and directory filters (and
  their counts after adding/removing torrents, moving one, changing the
  destinations' labels), and the search text, alone and combined;
- the selection: the general panel (seeders, leechers, status), the
  details lists, the properties dialog (its info page: status, ETA, last
  activity, sizes, error), the move dialog's title, the remove dialogs'
  name, the context menu's radio items, copying the magnet link;
- the "torrent added" and "torrent completed" notifications;
- the preferences' lists (destinations, custom headers): adding starts
  editing the first column, Enter or a double click edits, editing both
  columns, up/down, delete, and that OK saves them (also while a cell is
  still being edited) and reopening shows them.

Open observations from that pass, not investigated:

- `Gdk-CRITICAL: gdk_surface_thaw_updates: assertion
  'surface->update_freeze_count > 0' failed`, a few times while opening and
  closing the Remote Preferences and Statistics dialogs (Escape, the close
  button). It didn't reproduce in a dedicated run on the base commit
  (13fdfd0) or on 936a848, so it's intermittent, likely in GTK's macOS
  backend, and maybe older than this branch.
- The add dialog's file tree of a multi-file torrent starts below the
  torrent's own folder (`bencode.rs` and `FilesModel::set_tree`), while the
  server's file list includes it.
- The properties dialog's "Running time" shows the date the torrent was
  added, and "Remaining time" is empty for a complete torrent (check the
  labels against what `info_page_update` sets).
- After adding or deleting a tracker, the remaining tracker's Last
  Announce and Last Result are blank until the next update (the server
  gives the trackers new IDs).
- The Delete key alone didn't remove a torrent in the test (Cmd+Backspace
  did); possibly only the test tool's key mapping.
