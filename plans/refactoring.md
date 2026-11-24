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
- `src/item.rs` — the untyped column `Item`. `src/torrent_model.rs`,
  `src/files.rs`, `src/peers.rs`, `src/trackers.rs`, `src/state_selector.rs`
  — models of Items with `COL_*` constants.
- `src/tree_view.rs` — a GtkTreeView-like widget on GtkColumnView.
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

**Progress:** the state selector's rows are done (`StateRow` in
`src/state_selector.rs`, with its count label bound to the `count`
property): use it as the pattern for the property declarations.

**Next: let `TreeView` read columns from properties.** The other models are
shown with `TreeView`, which knows rows only as `Item`s and columns only as
model column indexes (about 60 places in `tree_view.rs`, and 27 calls from
outside). Generalize it once, so the models can move over one by one:

- `ColumnDesc` gets a value source instead of `model_column`/
  `model_column_extra`: e.g. `enum Source { Column(usize), Property(&'static
  str) }` for the value and the optional extra (icon or file ID). A
  property's value is read with `obj.property_value(name)` and converted
  into the cell value (`Val` can stay as that, until no model uses `Item`).
- `CellState::refresh()` and `cell_bind()` read through the source, and
  watch `column-changed::cN` for a column or `notify::<name>` for a property.
- Rows become `glib::Object` in the API (`selected_items()`,
  `first_selected()`, `select_item()`, `item_at()`, `ActivateFn`, `EditFn`,
  `start_editing()`), with the callers downcasting; `item::row_item()`
  unwraps a `TreeListRow`.
- Sorting compares the two rows' source values (now `Item::compare` by
  index); type-ahead search reads its column through a source.
- "Start editing once shown" is a field of `Item` (`start_edit_column`);
  keep it in the view instead (the row object and the column ID).
- Persistence: the sort column is saved as the model column index
  (`KEY_TV_SORT_COL`, `persist()`/`restore_sort()`). Keep a numeric ID per
  column, the old `COL_*` value, so saved settings stay valid; or save the
  column's string ID (`ColumnDesc::id`) and accept that existing sort
  settings reset once.

Then, per model: a GObject with properties (as `StateRow`), the store's
item type, the model's update code setting properties instead of columns,
the view's `reg_column()` calls using property sources, and the model's
other users (`files::set_wanted()`, `TrackersView`, `FilesView`'s rename,
`PeersModel`'s host lookups, `torrent_view::RowValues`, the window's filter
and the properties dialog, which read `tm::COL_*`). The torrent object can
hold the `Rc<Torrent>` and expose the derived values (flags, status text,
icon, shortened directory, first tracker host, peer sources, ratio) as
properties; `TorrentRow` then binds to them and `RowValues` goes away.

### 4. Split `tree_view.rs` (1,600 lines)

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
