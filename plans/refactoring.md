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
- `src/item.rs` — the untyped column `Item`, still used by
  `src/torrent_model.rs` (with `COL_*` constants) and the preferences' lists
  (`PersistentTreeView` in `dialogs/preferences.rs`). The other models are
  GObjects with properties: `StateRow` (`state_selector.rs`), `PeerObject`
  (`peers.rs`), `TrackerObject` (`trackers.rs`), `FileObject` (`files.rs`).
- `src/tree_view.rs` — a GtkTreeView-like widget on GtkColumnView; its
  columns read an Item column or an object property (`Source`).
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

### 3. Typed list items instead of the column store — the biggest gain

`Item` copies GTK 3's GtkListStore: a `Vec<Val>` of any types, `COL_*`
index constants (45 for a torrent), `item.int64(COL_X)` reads, a
`column-changed::cN` signal per column, and `TreeView` columns registered
by model column index (`reg_column(ColType, COL_X as i32, …)`), with sorting
by column index (`Item::compare`) and type-ahead by column.

Replace it with GObjects with properties (`#[derive(glib::Properties)]`):
`TorrentObject` (holding `Rc<Torrent>` plus the derived values: flags,
status text, icon, shortened directory, first tracker host, peer sources,
update serial), `PeerObject`, `TrackerObject`, `FileObject` (with children
and parent for the tree), and the state selector's row. Then:

- views bind cells to properties (or `notify::<prop>`) instead of
  `connect_column_changed(COL_X)`;
- `TreeView` columns are registered with a property name (or a getter
  closure) and a sort key, instead of a model column index;
- `Val`, `new_*_item()`, `RowValues` in `torrent_view.rs`, `TorrentItemExt`
  and most `COL_*` constants go away.

Do it one model at a time, smallest first: state selector rows, peers,
trackers, files, and the torrents last.

**Progress:** done: the state selector's rows (`StateRow`), `TreeView`
reading columns from properties, the peers (`PeerObject`), the trackers
(`TrackerObject`, with the editable announce URL) and the files tree
(`FileObject`, also in the add dialog and the rename dialog). Left: the
torrents (below), and the preferences' lists (`PersistentTreeView`: two
editable string columns of Items, whose keys come from the dialog; small,
could be a GObject with two string properties or stay on Items until the
torrents are done, then `Item` goes).

How `TreeView` works now (`src/tree_view.rs`):

- A column's value and its extra (icon name, file ID) come from a
  `Source`: `Column(usize)` of an `Item` or `Property(&'static str)` of the
  row object, read as a `Val` (`Val::from_value()` converts strings, i64,
  i32, u32, f64 and bools). The cells watch `column-changed::cN` or
  `notify::<name>`. `reg_column()` registers an Item column,
  `reg_property(ctype, key, property, header, id, flags)` a property one;
  `set_extra()` and `set_search_column()` take either (a `usize` or a
  `&'static str`).
- Each column has a numeric `key`, which is what the sort column is saved
  as (`KEY_TV_SORT_COL`) and the target of the sort menu's actions. For
  property columns it's the model column index they had as Items (written
  as numbers at the `reg_property()` calls, see "the keys are the model
  columns…"), so saved sort settings keep their meaning. A saved key that
  isn't registered sorts by that column of Items, as before.
- Rows are `glib::Object`s in the API: `selected_items::<T>()`,
  `first_selected::<T>()`, `select_item()`, `item_at()`, `start_editing()`;
  `set_activate()` and `set_edit()` take closures over `&T`.
  `tree_view::row_object()` unwraps a `GtkTreeListRow`.
- The cell to start editing once it's shown is kept in the view
  (`imp::TreeView::start_edit`: a weak row object and column), not in the
  row.
- `is_column_showing()` takes the column's string ID.

Setting a GObject property notifies even if the value doesn't change
(`Item::set` didn't), so the models set values with `util::set_changed()`,
which compares first; otherwise every update would refresh every bound
cell. Bookkeeping properties nobody shows (`serial`) use the plain setters.

**Next: the torrents.** `TorrentObject`, holding the `Rc<Torrent>` plus the
derived values (flags, status text, icon, shortened directory, first
tracker host, peer sources, update serial) as properties. Then
`torrent_view.rs`'s `reg_column()` calls become `reg_property()` with the
`COL_*` values as keys, `TorrentRow` binds to the properties and
`RowValues` goes, and the other users of `tm::COL_*` (about 75 places: the
window's filter and `first_selected`, the properties dialog, the general
panel, the state selector's counting, `TorrentItemExt`) read properties.
Then `Item`, `Val::Torrent`, `Source::Column`, `reg_column()` and the Item
fallback in `TreeView::sort_func` go (`Val` itself can stay as the cell
value, or become a smaller enum). Sorting a property column reads both
values with `property_value()` for each comparison (a string copy for text
columns); fine for the details lists, but measure with a few thousand
torrents, and give `ColumnDesc` a typed compare closure if it's slow.

### 4. Split `tree_view.rs` (1,700 lines)

After (or along with) step 3: separate the cell widgets (factories,
editable labels, the wanted check box, the priority cell), the persistence
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

## Not yet verified in the UI

The typed list items (step 3: `TreeView`'s sources, peers, trackers, files)
weren't clicked through either. Worth checking: sorting each details list
by every column (and that a saved sort survives a restart), type-ahead in
the peers (host) and files (name) lists, the host lookup when the Host
column is shown, adding a tracker (the new row starts editing), editing
and deleting trackers, the files' wanted check boxes and priority clicks
(directories show mixed, and follow their files), the files' context menu,
renaming a file, the add dialog's file list and "Apply to all", and the
preferences' lists (adding starts editing, Enter edits, up/down, delete).

These commits were not clicked through (the check was stopped early):
the connection refactor (`client/connection.rs`), the First Tracker column
and torrent row status fixes, `DetailLists`, and the typed `Torrent`,
`Session` and small results. Worth
a subagent UI pass: connect/disconnect/reconnect, failures while connected
(stop the daemon) and while connecting (bad port), periodic updates, the
torrent context menu's limits and priority, remote preferences (every
field loads and saves; the free space and the statistics show), removing
and adding torrents, the details pages and the properties dialog, the
First Tracker column and the sidebar's tracker filter.
