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
- `src/json.rs` — the lenient `value_as_i64()`-style getters, for the
  preferences file and the responses.
- The list models hold GObjects with properties: `TorrentObject`
  (`torrent_object.rs`, in the `TorrentModel` of `torrent_model.rs`),
  `StateRow` (`state_selector.rs`), `PeerObject` (`peers.rs`),
  `TrackerObject` (`trackers.rs`), `FileObject` (`files.rs`) and
  `PersistentRow` (the preferences' lists, `dialogs/preferences.rs`).
- `src/tree_view/` — a GtkTreeView-like widget on GtkColumnView; its
  columns show a property of the row objects. `mod.rs`: the widget, model
  and selection, `set_list_factory()`; `cell.rs`: the cells' widgets
  (`CellWidget`), their factory and binding; `column.rs`: `ColType`, the
  `COLUMN_*` flags, `ColumnDesc`, registering, showing/hiding and the
  header menus; `editing.rs`: editable cells (`start_editing`,
  `stop_editing`); `sort.rs`: sorting, `SortKey`, the sort indicator and
  `sort_menu()`; `persist.rs`: saving/restoring columns, widths and sorting
  (`persist`, `setup_columns`, `restore_sort`); `rows.rs`: finding rows for
  context menus (`item_at`, `point_in_header`, `focus_rect`,
  `context_menu_for_rows`, `mark_row_widget`); `expand.rs`: the files
  tree's expand/collapse (all, and the keys); `search.rs`: type-ahead,
  and the view's keys (one capture-phase controller: the expand keys, then
  the search);
  `value.rs`: `Val` (a string, i64 or f64 property) and `format_value()`.
- `src/type_ahead.rs` — `TypeAhead` (the typed text and its timing) and
  `is_shortcut()`, shared by the tree view and the state selector.
- `src/util.rs` — also the list helpers for any object type:
  `objects::<T>(model)`, `remove_where::<T>(store, stale)`, and
  `set_changed(obj, property, value)`; context menus
  (`add_context_menu`, `popover_menu_popup`, and `present_popovers` for the
  `size_allocate` of widgets that parent them).
- `src/state_selector.rs` — the sidebar; each `StateRow` has a `Filter`
  (`All`, `State(flags)`, `Tracker(host)`, `Directory(dir)`), and
  `StateSelector::filter()` is the selected row's.
- `src/main_window.rs` — the window: layout, wiring, actions, saving its
  size and panes, notifications.
- `src/torrent_list.rs` (`TrgTorrentList`, `src/ui/torrent-list.blp`) — the
  torrent list: the tree view (`torrent_view.rs`), its filter by the
  sidebar's `Filter` and the search text, the search entry (defined in the
  list's template, shown in the window's toolbar), "Not Connected" and "No
  Results" over the list, and the context menu (`torrent_menu.rs`).
- The view settings (sidebar, its directory/tracker filters and order, the
  details, the style) live in the preferences: the menu's stateful actions
  (`menu_bar.rs`) and the preferences dialog only set them, and the widgets
  follow `Prefs::connect_changed` (`MainWindow::follow_view_prefs`, the
  state selector's `build()`, `torrent_view.rs` for the style).
- `src/details.rs` — `Details` (`TrgDetails`, `src/ui/details.blp`), the
  notebook below the torrent list showing the selected torrent: the General
  page (`GeneralPanel`, `general_panel.rs`) and the lists; and
  `DetailLists`, the trackers/files/peers lists of a torrent, which the
  properties dialog shows too.
- `src/dialogs/` — dialogs; `dialog.rs` is a GtkDialog-like base class
  (the layout, Escape, and the `dialog.ok`/`dialog.cancel` actions, which
  respond with `Response::Ok`/`Response::Cancel`; and `add_app_shortcuts`,
  the `app.` actions' accelerators, as the dialogs don't belong to the
  application); `settings.rs` has the
  `settings_object!` macro (a GObject whose properties are fields of a
  struct, which the settings dialogs bind their widgets to) and `Closures`,
  the templates' closure functions (`$either`, `$both`).
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
(profiles, destinations, custom headers) and the responses. (`JsonWidgets`,
which bound dialog widgets to request keys, loaded from `Torrent::to_obj()`
and `Session::to_obj()`, went in step 9.)

### 3. Typed list items instead of the column store — done

`Item` (`src/item.rs`) copied GTK 3's GtkListStore: a `Vec<Val>` of any
types, `COL_*` index constants (45 for a torrent), `item.int64(COL_X)`
reads, a `column-changed::cN` signal per column, and `TreeView` columns
registered by model column index. It's gone; every list model holds
GObjects with properties (see "Where things are"), and `TreeView` columns
show a property.

How `TreeView` works now (`src/tree_view/`):

- `reg_property(ctype, key, property, header, id, flags)` registers a
  column showing a property of the row objects; `set_extra()` (the icon
  name, the file ID) and `set_search_column()` take property names too.
  The cells read the property (as a private `Val`: strings, i64 and f64,
  the only types the row objects show; another type panics) and watch
  `notify::<name>`.
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
  (Since step 4, the activate and edit closures no longer get the view:
  `set_activate(|file: &FileObject| ..)`, `set_edit(|row, done, text| ..)`.)

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
(`TorrentList::visible_func`), sorting, the stats, the sidebar's counting.
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

### 4. Split `tree_view.rs` — done

`src/tree_view.rs` (1,800 lines) is the `src/tree_view/` module (see
"Where things are"); the first commit (0052b61) only moved code. Then:

- The type-ahead of the tree view and the state selector shares
  `type_ahead::TypeAhead`; each still matches its own way (the sidebar
  ignores spaces and matches name prefixes only; the tree views match a
  prefix, else a substring). Both widgets' `size_allocate` call
  `util::present_popovers`.
- The activate/edit callbacks lost their unused `&TreeView` argument (and
  the cells their reference to the view). `ColumnDesc::set_editable()` is
  gone (`COLUMN_EDITABLE` alone makes a column editable), as is the
  `COLUMN_SHOWING` flag: a column is showing while it has its column view
  column, and the flags no longer change after registering (the remaining
  ones were renumbered; they're never saved). `ColumnDesc`'s fields are
  private to the module.
- `Val` is `Str(String) | I64 | F64`; a cell's widgets are a `CellWidget`
  enum instead of downcasts; the cell factory is in `cell.rs`; each part
  sets itself up (`setup_sorting`, `setup_column_actions`,
  `setup_type_ahead`, `setup_expand_keys`); `find_shown()` finds the
  column of a column view column; the unused sort model field and
  `prefs()` getter are gone.

Left as it was: `reg_property()`'s name and arguments, the `ColType` and
numeric sort keys, the `DATA_CELL`/`DATA_LIST_ITEM` object data, the
popover positioning for the keyboard (`util::popover_menu_popup` asks
`TreeView::focus_rect()`, the state selector does its own), and the two
`list.scroll-to-item` helpers.

### 5. The torrent filter as an enum — done

The sidebar packed its tracker and directory filters into the same `u32`
as the torrent status flags (`FILTER_FLAG_TRACKER = 1 << 14`,
`FILTER_FLAG_DIR = 1 << 15`), and the window decoded that plus the
selected row's name. Now each `StateRow` holds a `Filter` (a plain field,
not a property; set once by `StateRow::new`), `StateSelector::filter()`
returns the selected row's (`All` if none), the signal is `filter-changed`
(no arguments), and `MainWindow::visible_func` matches on the window's
`filter` field. `Torrent::has_tracker` takes a `&str`. The selected row's
saved index (`STATE_SELECTOR_LAST`, `StateRow::index`) is unchanged.

`TORRENT_FLAG_*` stayed plain `u32` constants: `flags` is a GObject
property of `TorrentObject` (the `torrent_values!` macro) and sorts through
`SortKey`, so a `bitflags` type would need a ParamSpec and a `SortKey`
impl, plus rewriting about 50 `& FLAG != 0` tests, for little gain.

### 6. The preferences as the single source of truth for view settings — done

Before, the preferences dialog's check boxes called the sidebar and the
window directly (a preview, saved on OK), while the menu's stateful
actions wrote the preference and the window followed the actions' state.
Now:

- The menu's view actions (`view_item_new`, and the style action) only set
  the preference in `change_state`; their state follows the preference.
  `filter-dirs`, `filter-trackers` and `directories-first` are enabled
  while `KEY_SHOW_STATE_SELECTOR` is true (following the preference, no
  longer the `view-states` action).
- The window shows the sidebar and the details as the preferences say
  (`follow_view_prefs`); the state selector follows its three options
  itself, and its setters (`set_show_dirs`, `set_show_trackers`,
  `set_directories_first`) are private and do nothing if the value is
  unchanged (they used to rebuild the rows every time). The window used to
  apply the sidebar's options only while their actions were enabled; the
  selector now always follows them (only the dialog could change them
  while the sidebar was hidden, and it applied them directly).
- The preferences dialog's five view check boxes set the preference as
  they're toggled (the preview) and follow it while the dialog is open (a
  menu item or shortcut used meanwhile shows there, and OK no longer
  undoes it). It no longer knows the main window (`new()` takes any parent
  window; `notebook_set_visible()` is gone, `state_selector()` private).
- **Behaviour change:** closing the dialog without OK (Close, Escape, the
  window's close button) restores the values the view settings had when it
  opened. Before, the preview stayed applied but unsaved, out of step with
  the menu's check items (the next shortcut seemed to do nothing) and gone
  after a restart. Keeping that would now leave the preview in the
  preferences, which the main window saves on quit. For the same reason
  `save_window_state()` closes an open preferences dialog first.

### 7. A `TorrentList` widget — done

The torrent list is a composite widget, `TorrentList` (`src/torrent_list.rs`,
template `src/ui/torrent-list.blp`), a template child of the main window
(the `hpaned`'s end child). `main_window.rs` went from 1,062 to 879 lines.

- Its template has the overlay with "Not Connected" (and its stack:
  connect, edit profiles, connecting) and "No Results", the context menu's
  sections (`torrent_actions_menu`, `torrent_queue_menu`), and the search
  entry as a separate top-level object: the window puts it in its toolbar
  (`list.search_entry()`), makes it capture the window's keys, and focuses
  it on Ctrl/Cmd+K and F (`focus_search()`) from its `key_pressed`, which
  keeps forwarding the app's shortcuts to text fields. The entry's signals
  are connected in code (`setup_search`), not as template callbacks: the
  entry isn't inside the list, so a callback couldn't find it.
- `set_client()` makes the tree view (config id "TrgTorrentTreeView", as
  before) and its filter. The API: `set_filter(Filter)` (the window passes
  the sidebar's on `filter-changed`), `refilter_and_resort()` (after each
  update), `set_connection(state, profile_name)` (sensitivity, "Not
  Connected", "No Results"), `set_queues_enabled()`, `selected_id()`,
  `selected_ids()`, `first_selected()`, `persist()`, `tree_view()` (for
  the View menu's sort menu), and the signals `selection-changed` and
  `row-activated`.
- `selected_id()` is the first selected torrent's ID as of the last
  `selection-changed`, which the list emits on the selection's
  selection-changed and when items-changed changes the first selected
  torrent (the window used to keep that ID; `MainWindow::selected_torrent_id`
  now reads it from the list).
- `torrent_menu::popup` takes the client, the tree view and the current
  torrent's ID instead of the window; the properties and move dialogs take
  the selected IDs instead of the tree view.
- The list disposes its template, which clears its template children while
  the client's model (and the sidebar following it) can still change:
  `refilter` and `update_no_results` return when the entry is gone.

### 8. A `Details` widget — done

The goal: the details notebook below the torrent list (the General page,
`GeneralPanel`, and the trackers/files/peers pages, `DetailLists`) was part
of the main window's template, and the window filled and cleared it: it
showed the selected torrent (`update_selected_torrent_notebook`), cleared
it (`torrent_scrub`), set its sensitivity on connecting and disconnecting,
appended the lists' pages in `construct()` and persisted their layouts in
`save_window_state()`. Make it one composite widget, like `TorrentList` in
step 7, with its own Blueprint template, that fills and clears itself: the
window only gives it the client, the selected torrent with the update mode,
and the connection state, and asks it to persist. The menu bar's torrent
actions, enabled while a torrent is selected, stay the window's. The
properties dialog's lists (their own config suffix) keep working.

Keep the behaviour: which page shows, the notebook following the "torrent
details" view preference, the vpaned's position saved and restored, the
lists' columns and sorting (config ids unchanged), the files tree built in
a thread for big torrents, keyboard focus and the lists' context menus.

What was done (`main_window.rs` went from 879 to 858 lines):

- `Details` (`src/details.rs`, template `src/ui/details.blp`) is a template
  child of the main window, the vpaned's end child. Its template has the
  notebook with the General page; `set_client()` makes the lists (config
  ids "TrgTrackersTreeView", "TrgFilesTreeView", "TrgPeersTreeView", as
  before), appends their pages after it and makes them insensitive until
  connected. The API: `set_client(client)`, `set_torrent(Option<&TorrentObject>,
  mode)` (`None` clears it), `set_connection(state)` (sensitive while
  connected, cleared once disconnected) and `persist()`.
- The window's `show_selected_torrent(mode)` (on the list's
  `selection-changed` with `First`, after each update with its mode) looks
  up the selected torrent (`selected_torrent()`, which `copy_magnet_link`
  uses too), enables the torrent actions while there's one and passes it to
  the details; on disconnecting it disables them. The vpaned (its position
  saved and restored) and the details following `KEY_SHOW_NOTEBOOK`
  (`follow_view_prefs`, now hiding the `Details` widget rather than the
  notebook) stay the window's.
- `DetailLists` stays a type of its own: the properties dialog shows its
  pages around its own info page, in another order (files, peers,
  trackers), with the config suffix "-dialog", and updates them from the
  model itself. Its fields and `clear()` are private now.
- The files view no longer keeps the main window: the rename dialog's
  parent is the view's window, or the window that one is transient for
  (`FilesView::dialog_parent`), so the main window both from the details
  and from the properties dialog, as before; `FileRenameDialog::new` takes
  a `gtk::Window`.
- Like the torrent list, the details dispose their template; the General
  page is then gone, and `set_torrent` and `set_connection` skip it
  (`Details::general()`), should the torrents change after that.

### 9. The settings dialogs with data binding — done

The goal: the remote preferences (`dialogs/remote_prefs.rs`,
`ui/remote-prefs-dialog.blp`) and the properties dialog's limits page
(`dialogs/props.rs`) list `(widget, key)` pairs and load and save them with
`JsonWidgets` (`dialogs/json_widgets.rs`) from `Session::to_obj()` and
`Torrent::to_obj()`; the local preferences (`dialogs/preferences.rs`) do the
same with `PrefWidget` and the preferences' keys and scopes. Each setting is
named in the Blueprint file, a `#[template_child]` field and the table.
Instead, give each dialog a settings GObject whose properties are the
settings, and bind the widgets to it in Blueprint (`active: bind
settings.dht-enabled bidirectional;`), dropping the template children and
tables that only served loading and saving. No libadwaita (user decision).

Approach, one dialog per commit (as planned):

- The remote preferences first: a settings object generated from the
  `Session` fields with a macro (like `torrent_values!`), made from the
  client's session; OK sends a `session_set` with only the fields that
  changed. The odd cases (the alternate times as hours and minutes, the
  days of the week as check boxes, the encryption and transports
  drop-downs) bind with transform functions; the sensitivities that depend
  on two settings with closures.
- Then the properties dialog's limits page, from the `Torrent` fields.
- Then the local preferences: the profile's settings, the global ones, and
  the view settings (previewed as step 6 made them, restored on Close),
  also when switching profiles in the dialog.
- Remove `JsonWidgets`, `PrefWidget` and what becomes dead.

Keep the behaviour otherwise: what loads, what's saved and when (OK or
Close), the sensitivity dependencies, the optional rows' visibility, the
macOS fixed-size Settings window.

What was done (the three dialogs went from 1,661 to 1,496 lines, plus the
166 of `settings.rs`; the Blueprint files gained 82 lines of bindings):

- **The settings objects** are top-level objects of the dialogs' Blueprint
  files (`$TrgRemoteSettings settings {}`) and template children, so the
  widgets bind to them by ID: Blueprint only allows flags like
  `bidirectional` on a single lookup, so `bind template.settings.x
  bidirectional` doesn't compile. Their types are registered in
  `class_init` (`ensure_type()`).
- `settings_object!` (`dialogs/settings.rs`) defines one from fields of a
  struct, like `torrent_values!`: `RemoteSettings` holds a `Session`,
  `TorrentSettings` a `Torrent`. Each listed `field: Type` is the property
  "field" (with dashes); an `Option<T>` field is a T property (the default
  when None, Some once set), through the `Setting` trait. `load()` shows a
  struct (notifying every property) and `changes()` returns the listed
  fields that differ from it, as JSON named like the fields (serde's names,
  the protocol's keys).
- `LocalSettings` (`dialogs/preferences.rs`) isn't made by the macro: its
  properties are the preferences in the `SETTINGS` table (key, type,
  scope), named like their keys; `load(prefs)` and `save(prefs)` read and
  write all of them as `PrefWidget` did, so saving still writes every value
  (and emits `pref-changed` for each). The view settings preview and follow
  their preferences through its properties (`preview_view_prefs`); the
  lists (headers, destinations) are refreshed and saved beside it.
- Conversions: bindings with transform functions in Rust (`bind_property`
  with `transform_to`/`transform_from`) for the encryption and transports
  drop-downs (string or list ↔ position), the alternate times (minutes ↔
  hours and minutes spin buttons; each reads the other part from the
  settings, `settings::source_property`), the days' check boxes (a bit
  each) and the torrent's bandwidth priority drop-down (priority + 1).
  GBinding's default transforms cover i64 ↔ a spin button's f64 and i64 ↔
  a drop-down's u32 (the seed ratio mode). Sensitivities depending on two
  settings are closure bindings (`$either(...)`, `$both(...)`,
  `$stops_at_ratio(...)`); those on one check box stay bindings to it.
  Rows hidden without the server's support (transports, queue position,
  sequential download) and the blocklist's entry count stay set in code.
- **Behaviour changes:** the remote preferences' OK sends only the settings
  that changed, and nothing if none did (no session_set, so no session
  refresh either); it used to send all of them, plus the encryption. The
  properties' OK likewise: with several torrents, only what was edited is
  set on all of them, rather than every limit of the first torrent
  (including its queue position, which moved them all there). Nothing
  relied on resending: values the server normalizes (like a directory's
  trailing slash) now just aren't sent back. A value the widget clamps
  (out of a spin button's range) isn't written back to the settings, so
  it's sent or saved only once edited; before, the clamped value was sent
  or saved. A fractional value typed into an integer spin button is
  truncated by the binding, where `value_as_int()` rounded.
- Gone: `JsonWidgets` (`dialogs/json_widgets.rs`), `PrefWidget`, 69 widget
  template children, `Session::to_obj()`, `Torrent::to_obj()`, the JSON
  getters only they used, and the unused `SGET_*`/`FIELD_*` constants.
- Noticed, fixed later: the alternate times' minutes spin buttons went up
  to 69 (60 to 69 added an hour and more; 59 since c5d5ae5);
  `po/POTFILES.in` listed `src/protocol.rs`, which no longer exists, and
  not `src/protocol/torrent.rs` (32f275d).

### Smaller items — done or decided

- **CI** — the workflow is `.github/workflows/build.yml` (was `c-cpp.yml`;
  still named "CI"). The Linux and macOS jobs run `meson test` (which runs
  `cargo test` with meson's environment, so blueprint-compiler can be
  meson's git fallback, as on Ubuntu) and
  `meson devenv -C build cargo clippy ... --all-targets -- -D warnings`.
  Not on Windows, where the tests have never run (the translation test
  sets a locale that may not exist there). `formatting.yml` needed no
  change. The translation test failed under meson (and `meson dist`, which
  runs the tests): it looked in `TRG_LOCALEDIR`, the install prefix. build.rs
  now always compiles the translations into `OUT_DIR/locale`, which the test
  uses.
- **The `Dialog` base class — kept**, simplified: it gives eight dialogs
  their layout (the content area, the row of buttons from `[action]`
  children), Escape, closing as a response, and actions whose enabling
  makes the buttons insensitive. Plain `gtk::Window`s would repeat all that
  in each template and class. What went: GtkDialog's five response IDs;
  every dialog only told a confirmation from anything else, so there are
  `Response::Ok` (`dialog.ok`: OK, Add, Move, Rename) and `Response::Cancel`
  (`dialog.cancel`: Cancel, Close, closing the window).
- **Typed errors** — `client::SettingsError` (`NoHostname`, now "No
  hostname set" instead of the untranslated "Bad hostname.", and `BadUrl`)
  and `add_torrents::AddError` (`Read`: names the file, with the C
  version's "Unable to open torrent file: %s", also in the add dialog).
  They implement `Display` by hand: **xgettext doesn't extract strings in
  attributes**, so `#[error("{}", tr("..."))]` doesn't work; rpc::Error's
  translated messages moved into functions for that reason.
- **gettext — left as hand-written FFI.** `gettext-rs` (gettext-sys 0.27)
  uses the system's gettext only with glibc/musl, MinGW and FreeBSD; on
  macOS it builds GNU gettext from source unless `GETTEXT_DIR` (or the
  `GETTEXT_*_DIR` variables) are set, which the meson and plain cargo builds
  would then have to set. build.rs already links GLib's libintl, so the FFI
  (five functions) stays.
- **The About dialog's Escape on macOS — not fixed, cause unclear.**
  GtkAboutDialog binds Escape to `window.close` itself (GTK 4.20
  `gtkaboutdialog.c`), the same binding `TrgDialog` has, and Escape closes
  those on macOS. The difference: the About dialog is modal, which the GTK
  macOS backend makes a child window at `NSModalPanelWindowLevel`, so the
  key may not reach it (not the key window). Worth trying in a UI pass:
  whether it closes with Escape after a click into it, and without
  `set_modal(true)` in `main_window.rs`; a shortcut controller wouldn't
  help if the key never reaches the dialog.

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

After step 4 (commits 0052b61 to c6a9924), not clicked through yet; check,
besides the list below:

- type-ahead in the files list (prefix, then substring; a pause starts
  over; with spaces), the peers list (by host; needs real peers), and
  the sidebar (prefix only, spaces ignored); that typing
  doesn't trigger the files tree's +, -, * and / keys while a search is
  being typed, and that they (and Shift+Left/Right) expand and collapse
  otherwise;
- every context menu (torrents, files, trackers, sidebar, column headers)
  opening at the pointer and from the keyboard (Shift+F10/Menu), sized
  right on the first frame;
- the column header menus: hiding a column, showing a hidden one after
  another, Unremovable columns (trackers' Tier and Announce URL) not
  hideable, the last shown column not hideable; that hidden columns,
  order and widths survive a restart (also with settings from before);
- sorting by clicking headers and by View > Sort, ascending and
  descending, in the torrents, files, trackers and peers lists;
- the files list: the check box (also on a directory: mixed), clicking the
  priority (normal, high, low), the file icons and folder icons, the
  expanders;
- editing the trackers' announce URL (select, then click; Enter; Escape)
  and the preferences' lists (as below);
- the Transmission/compact styles (`set_list_factory`) and switching back
  to the classic columns.

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

After steps 5 and 6 (commits 41e121f to d740b0b), not clicked through yet;
check:

- the sidebar's filters: each state (and Error, Queue Down/Up when the
  server has queues), a tracker, a directory, combined with the search
  text; that the selected row is restored after a restart (a state, and a
  tracker or directory falling back to All); that a selected tracker or
  directory row going away (its last torrent removed or moved, the filter
  switched off) shows all torrents;
- the View menu's Sidebar, Directory Filters, Tracker Filters, Directories
  First and Torrent Details items and their shortcuts (F9 or Ctrl+Cmd+S on
  macOS, F3, F4, F5): the widgets change and the check marks follow; the
  three sidebar options are disabled while the sidebar is hidden; the style
  radio items (Transmission, classic, compact) still switch and stay
  checked;
- the preferences dialog's View check boxes: each toggles the window
  straight away and the menu's check marks follow; the sidebar options are
  insensitive while Sidebar is unchecked; OK keeps them (also after a
  restart); Close, Escape and the window's close button restore what they
  were when it opened; toggling a View menu item (or its shortcut) while
  the dialog is open updates its check box, and OK keeps it; switching
  profiles in the dialog, then Close, restores them too;
- closing the main window (or Ctrl+Q / Cmd+Q while it has the focus; from
  the dialog, that did nothing; since 1f593bb it quits, see below) while the dialog is open with an unconfirmed change:
  after a restart the old value shows.

Observations from that pass, investigated after step 7:

- `Gdk-CRITICAL: gdk_surface_thaw_updates: assertion
  'surface->update_freeze_count > 0' failed` while opening and closing the
  Remote Preferences and Statistics dialogs: the app never freezes or thaws
  surfaces; the macOS backend does (`gdkmacossurface.c`: a freeze per
  requested frame, thawed when it's presented if the surface is mapped, or
  when the frame is cancelled, and a thaw on the first show), so a window
  hidden or destroyed while it awaits a frame can unbalance it. The
  dialogs are destroyed from their response handler (Escape: from
  `close-request`), as GtkDialog applications did. Left as GTK's.
- The add dialog's file tree of a multi-file torrent started below the
  torrent's folder: fixed, the files are under the folder (the torrent's
  name) as in the server's list (6d826dd); the file indexes are unchanged.
- The properties' "Running time" showed the date added (as in the C
  version): it's labelled "Added" now; "Remaining time" shows "N/A"
  without an ETA, like the general panel; "Last activity" says "Active
  now" while the torrent transfers, otherwise the activity date (128f0f9).
- After adding or deleting a tracker, Last Announce/Last Result were
  blank: that's the server. Wire-checked with Transmission 4.1.4: after a
  torrent_set of `tracker_list`, every tracker (also the unchanged one)
  has a new ID, `has_announced: false`, `last_announce_time: 0` and an
  empty `last_announce_result`, still a few seconds later, until it
  announces again.
- Delete didn't remove a torrent: the test tool's `key delete` sends key
  code 51, the Mac's Delete key, which is BackSpace in GTK; `win.remove` is
  Delete (forward delete, `key forwarddelete`) and Cmd+BackSpace on macOS,
  like Finder. Not a bug.
- From the task list: the old header kept its sort arrow after sorting from
  View > Sort: `gtk_column_view_sort_by_column()` replaces the sort
  columns but notifies only the new column's header; the tree view clears
  the sort first now (2da02b9).
- From the task list: the sidebar's Active counted every downloading
  torrent. Transmission's clients (GTK `Torrent.cc`, web `torrent.js`)
  count a torrent active while peers send to it or get from it (web: also
  web seeds) or while it's checked; the flag does that now (2e4efbe,
  behaviour change). The hidden Peers T/I/E/H/X/L/R column shows the peer
  sources while a torrent is downloading or seeding.

After step 7 (commits 49a14ad to 8f59a41), not clicked through yet;
check:

- the window looks as before: the search entry at the right of the
  toolbar, the torrent list beside the sidebar, "Not Connected" (Connect,
  Edit Profiles… without a host, the spinner while connecting) and "No
  Results" over the list, clipped to it when the list is narrow;
- which widget has the focus on startup, as on the base commit;
- typing anywhere in the window starts a search (not in a text field, the
  sidebar or the details' lists, which type ahead), keeping the first
  character; Ctrl/Cmd+K and Ctrl/Cmd+F focus the entry; Escape clears it,
  and a second Escape returns the focus to the torrent list; Delete in the
  entry edits the text and doesn't remove torrents;
- the search and the sidebar's filters together, and "No Results" only
  while connected with search text and nothing matching;
- the selection: the details and the general panel follow it, also when
  the selected torrent is filtered out or removed; double click or Enter
  opens the properties (of all selected torrents); the remove dialogs'
  name and count; the move dialog's title;
- the context menu with the pointer and the keyboard (Shift+F10/Menu), its
  queue items only while the server has queues, its limits and priority
  radio items showing the first selected torrent's values;
- the list insensitive while disconnected and connecting, sensitive once
  connected; the torrent list's columns, widths and sorting surviving a
  restart; the View > Sort menu.

After the smaller items and the observations (commits dcc13e2 to 2e4efbe),
not clicked through yet; check:

- every dialog's buttons: OK/Add/Move/Rename apply, Cancel/Close and the
  window's close button and Escape don't; Add, Move and Rename insensitive
  while their entry is empty; the preferences dialog's view settings
  restored on Cancel/Close (TrgDialog's responses were reduced to OK and
  Cancel);
- connecting to a profile without a host: "No hostname set", then the
  preferences open;
- adding a .torrent file that can't be read (e.g. `chmod 000`), with and
  without the add dialog: the message names the file;
- the add dialog with a multi-file torrent: the tree starts with the
  torrent's folder, expanded; unchecking files and setting priorities, then
  adding: the server's files have those (by index), also in subfolders;
  a single-file torrent as before;
- the properties' info page: "Added" with the date, "Remaining time" with
  the ETA while downloading and "N/A" otherwise, "Last activity": "Active
  now" while transferring, a date when idle;
- sorting by a header, then by another column from View > Sort: only the
  new header has an arrow (also ascending/descending, and back to a
  header click); restored sorting after a restart shows one arrow;
- the sidebar's Active count and filter: a downloading torrent without
  peers isn't active, one receiving or sending is, a checking one is;
- the About dialog and Escape on macOS (see "Smaller items").

After step 8 (commits 67518a7 to 9bf8e76), not clicked through yet; check:

- the details look as before below the torrent list: General first (also
  after a restart), then Trackers, Files, Peers; F5 / View > Torrent Details
  and the preferences' check box hide and show them, the torrent list then
  taking the height; the divider's position surviving a restart (and
  hiding/showing the details);
- the details following the selection (the General page and each list),
  updating with the periodic updates, cleared when nothing is selected,
  the selected torrent is filtered out or removed, and on disconnecting;
  insensitive while disconnected and connecting (the lists from startup),
  sensitive once connected;
- the torrent actions (menu bar and toolbar: Properties, Move, Remove,
  Verify, ...) enabled only while a torrent is selected, disabled after
  disconnecting; Copy Magnet Link;
- the lists' column layouts and sorting surviving a restart (main window
  and properties dialog separately); a torrent with many files (over 600)
  showing its tree;
- the lists' context menus (trackers: add/edit/delete; files: priorities,
  wanted, expand/collapse, Rename) with the pointer and the keyboard; the
  rename dialog from the details and from the properties dialog, both on
  top and renaming;
- the keyboard: Tab into the notebook's tabs and the lists, the lists'
  type-ahead, and that typing elsewhere still starts a search.

After step 9 (commits 7059eba to 90f441a), not clicked through yet; the
widgets' bindings and conversions were checked in a throwaway run building
the dialogs offscreen (no window shown); check in the app:

- the remote preferences show the server's values on every page (also
  the encryption, transports, alternate begin/end times and days, the
  blocklist's entry count); the transports row hidden with a server
  without them; the alternate limits insensitive unless "Alternate speed
  limits active" or "Alternate time range" is checked, and the times and
  days insensitive without the time range; the other spin buttons and
  entries following their check boxes;
- changing a few settings of each kind (a check box, a spin button, an
  entry, the encryption, the transports, a begin hour and an end minute, a
  day) then OK: the RPC log's session_set has exactly those (the times in
  minutes, the days as bits) and the server has them on reopening; OK
  without changes sends nothing; Close and Escape send nothing; Port test
  and blocklist Update still work and update their labels;
- the properties' limits page: the first torrent's values, the seed ratio
  limit sensitive only for "Stop seeding at ratio", the queue position and
  sequential rows shown as before (from piece: one torrent only, with its
  piece count as the maximum, sensitive while "Download sequentially" is
  checked); OK sends only what changed (RPC log), also with several
  torrents (each keeps its other limits and queue position); the
  bandwidth priority's three values;
- the local preferences: each field loads and saves (OK, then reopen;
  after a restart), the Full update spin button sensitive only with both
  "Update active torrents only" and "Full update every" checked, SSL
  validation following SSL; switching profiles saves the edited one and
  shows the other's fields (the name in the drop-down following the name
  entry), New and Delete; the View check boxes as after step 6 (preview,
  the menu following, restored on Close/Escape, kept on OK); the macOS
  Settings window still fixed-size and tall enough; the headers and
  destinations lists as before.

### Fixes from the click-through after step 9

A click-through on Linux (GTK 4, the `linux-ui-debugging` harness) found
these; fixed in commits a5e46d9 to c5d5ae5, written without running the
app:

- **Type-ahead with a space** (a5e46d9): typing "sub d" in the files list
  selected "sub dir", but the space also reached the row with the keyboard
  focus, whose binding toggled its selection (two rows selected), and the
  focus stayed on the row clicked before. The tree view's keys are one
  capture-phase controller now (`search.rs`): the expand/collapse keys,
  then the search, before the rows' bindings. A leading space isn't a
  search (it selects the focused row as before); once a search is being
  typed (`TypeAhead::is_typing`, less than a second since the last key)
  spaces are search text, as are the expand keys (+, -, *, /). A found row
  gets the focus too (`scroll_to` with `ListScrollFlags::FOCUS | SELECT`;
  GTK moves the keyboard focus only if it's in the list, otherwise just
  the list's focus row), also in the sidebar and for `select_item` (the
  trackers' Add, the preferences' lists). The controller leaves the keys
  to a cell being edited (a `gtk::Text` has the focus) and to the view's
  context menus.
- **Ctrl+Q (Cmd+Q) in a dialog** (1f593bb): the app's accelerators only
  work in the application's windows, and the dialogs aren't (on purpose:
  on macOS the menu bar's `win.` actions follow the application's active
  window, so they'd be disabled while a dialog had the focus). Each
  `TrgDialog` and the About dialog have a capture-phase shortcut controller
  with the `app.` actions' accelerators (`dialog::add_app_shortcuts`):
  Quit, and on macOS Close Window (Cmd+W) and Minimize (Cmd+M), which act on
  the focused window, and Preferences (Cmd+comma) if GTK sets it. The main
  window's `win.` shortcuts don't reach the dialogs. `app.quit` closes the
  main window (not `windows().first()`), which saves its state, closing an
  open preferences dialog first (restoring an unconfirmed view preview),
  and quits when destroyed.
- **The torrent context menu's queue items** — no change: they're added
  at each popup from `TorrentList`'s `queues_enabled`, which
  `MainWindow::session_updated` sets whenever the session's
  `download_queue_enabled || seed_queue_enabled` changes (both start
  true, as do the menu bar and the sidebar's queue rows, so a first
  session with queues off removes them). The session comes with every
  session refresh, and right after the remote preferences' OK (in the
  session_set's batch); a change made outside the app shows with the next
  refresh. Both queues off hides them; one on keeps them.
- **The startup focus** (b17089a): on main too, nothing chose it, so GTK
  gave it to the first widget that could take it, the toolbar's server
  picker, as the torrent list is insensitive until connected. The first
  connection now focuses the list (`MainWindow::focus_torrent_list`),
  unless a text field (a search typed while connecting) or a menu has the
  focus; later connections do if the focus widget went away (hidden, like
  the Connect button, or insensitive).
- **The alternate times' minutes** go up to 59 (c5d5ae5).

Not clicked through yet; check:

- the files list (and the add dialog's): click a row, type "sub d"
  quickly: only "sub dir" is selected and has the focus (Down moves to
  the row after it, Shift+Down extends from it); a space after a pause
  (over a second) toggles the focused row's selection as before (Ctrl+click
  style); +, -, * and / expand and collapse when not typing a search, and
  are search text right after a typed character; the peers list by host;
  the sidebar: typing selects and focuses the state (arrows continue
  from there), spaces ignored;
- editing a cell (trackers' announce URL, the preferences' lists): typing
  letters and spaces edits the text, also right after typing a search in
  that list and pressing Enter; the context menus' keyboard handling with
  a menu open (no search starts);
- the trackers' Add and the preferences' lists' Add/Up/Down: the row is
  selected (and shown) as before, and the keyboard focus doesn't jump
  into the list from the buttons;
- Ctrl+Q (Cmd+Q) from each dialog (local and remote preferences,
  properties, add, add URL, move, rename, statistics, About): the app quits
  and saves its state; from the preferences dialog with an unconfirmed
  View change, the old value shows after a restart; Ctrl+Q while the main
  window has the focus as before; the main window's shortcuts (Ctrl+C,
  Delete, F9, Ctrl+O...) don't act from a dialog (Delete and Ctrl+C edit
  its text fields); on macOS: Cmd+W closes and Cmd+M minimizes the focused
  dialog, the menu bar's items keep working while a dialog has the focus
  (e.g. View > Sidebar with the preferences open), and Cmd+Q from the
  menu and the keyboard quits with a dialog focused;
- the startup focus: once connected, the torrent list has it (Down selects
  a torrent, Enter opens its properties); typing during connecting goes to
  the search entry and keeps the focus there; starting disconnected and
  clicking Connect, or connecting from the server picker, focuses the
  list too;
- the context menu's queue items after turning both queues off and on in
  the remote preferences (right away after OK) and on the daemon directly
  (after the session refresh); the initial state with a server whose
  queues are off;
- the remote preferences' minutes spin buttons stop at 59.
