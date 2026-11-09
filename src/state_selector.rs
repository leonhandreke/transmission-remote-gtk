//! The sidebar of torrent states, trackers and directories to filter the
//! torrents by (trg-state-selector.c). Its rows count the torrents.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib};

use crate::client::Client;
use crate::i18n::tr;
use crate::item::{self, Item, Val};
use crate::prefs::{self, Prefs};
use crate::protocol::{self as p, get_str};
use crate::torrent_model::{self as tm, Stats, TorrentItemExt, TorrentModel};
use crate::tree_view;
use crate::util::{self, HostRegex};

pub const FILTER_FLAG_TRACKER: u32 = 1 << 14;
pub const FILTER_FLAG_DIR: u32 = 1 << 15;

/// The columns of the rows. A row without a name is the separator.
const COL_ICON: usize = 0;
const COL_NAME: usize = 1;
const COL_COUNT: usize = 2;
const COL_BIT: usize = 3;
const COL_SERIAL: usize = 4;
const COL_INDEX: usize = 5;

fn new_state_item(icon: Option<&str>, name: Option<&str>, bit: u32, index: u32) -> Item {
    Item::new(vec![
        Val::Str(icon.map(str::to_string)),
        Val::Str(name.map(str::to_string)),
        Val::I64(0),
        Val::U32(bit),
        Val::I64(0),
        Val::U32(index),
    ])
}

#[derive(Default)]
pub struct States {
    error: Option<Item>,
    all: Option<Item>,
    paused: Option<Item>,
    down: Option<Item>,
    seeding: Option<Item>,
    complete: Option<Item>,
    incomplete: Option<Item>,
    checking: Option<Item>,
    active: Option<Item>,
    seed_wait: Option<Item>,
    down_wait: Option<Item>,
}

mod imp {
    use super::*;
    use std::sync::OnceLock;

    pub struct StateSelector {
        pub scrolled: RefCell<Option<gtk::ScrolledWindow>>,
        pub list_view: RefCell<Option<gtk::ListView>>,
        pub selection: gtk::SingleSelection,
        pub store: gio::ListStore,
        pub search: RefCell<String>,
        pub search_time: Cell<i64>,

        pub flag: Cell<u32>,
        pub show_dirs: Cell<bool>,
        pub show_trackers: Cell<bool>,
        pub dirs_first: Cell<bool>,
        pub client: RefCell<Option<Client>>,
        pub trackers: RefCell<HashMap<String, Item>>,
        pub directories: RefCell<HashMap<String, Item>>,
        pub host_regex: HostRegex,
        pub n_categories: Cell<i32>,
        pub states: RefCell<States>,
    }

    impl Default for StateSelector {
        fn default() -> Self {
            let store = gio::ListStore::new::<Item>();
            Self {
                scrolled: Default::default(),
                list_view: Default::default(),
                selection: gtk::SingleSelection::new(Some(store.clone())),
                store,
                search: Default::default(),
                search_time: Cell::new(0),
                flag: Cell::new(0),
                show_dirs: Cell::new(false),
                show_trackers: Cell::new(false),
                dirs_first: Cell::new(false),
                client: Default::default(),
                trackers: Default::default(),
                directories: Default::default(),
                host_regex: HostRegex::new(),
                n_categories: Cell::new(0),
                states: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StateSelector {
        const NAME: &'static str = "TrgStateSelector";
        type Type = super::StateSelector;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for StateSelector {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("torrent-state-changed")
                        .param_types([u32::static_type()])
                        .build(),
                ]
            })
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for StateSelector {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.scrolled.borrow().as_ref() {
                Some(s) => s.measure(orientation, for_size),
                None => (0, 0, -1, -1),
            }
        }

        // The context menu is a popover of the selector, present it
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(s) = self.scrolled.borrow().as_ref() {
                s.allocate(width, height, baseline, None);
            }
            let mut child = self.obj().first_child();
            while let Some(c) = child {
                if let Some(p) = c.downcast_ref::<gtk::Popover>() {
                    p.present();
                }
                child = c.next_sibling();
            }
        }

        fn focus(&self, direction: gtk::DirectionType) -> bool {
            self.scrolled.borrow().as_ref().is_some_and(|s| s.child_focus(direction))
        }

        fn grab_focus(&self) -> bool {
            self.list_view.borrow().as_ref().is_some_and(|l| l.grab_focus())
        }
    }
}

glib::wrapper! {
    pub struct StateSelector(ObjectSubclass<imp::StateSelector>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl StateSelector {
    pub fn new(client: &Client, model: &TorrentModel) -> Self {
        let s: Self = glib::Object::new();
        *s.imp().client.borrow_mut() = Some(client.clone());
        s.build();

        let weak = s.downgrade();
        let m = model.downgrade();
        model.connect_state_change(move |changed| {
            let (Some(s), Some(model)) = (weak.upgrade(), m.upgrade()) else { return };
            s.update(changed);
            if changed & (tm::UPDATE_ADDREMOVE | tm::UPDATE_STATE_CHANGE) != 0 {
                s.stats_update(&model.stats());
            }
        });
        s
    }

    fn client(&self) -> Client {
        self.imp().client.borrow().clone().unwrap()
    }

    fn prefs(&self) -> Prefs {
        self.client().prefs().clone()
    }

    pub fn connect_state_changed<F: Fn(&Self, u32) + 'static>(&self, f: F) {
        self.connect_local("torrent-state-changed", false, move |args| {
            f(&args[0].get::<Self>().unwrap(), args[1].get::<u32>().unwrap());
            None
        });
    }

    pub fn host_regex(&self) -> &HostRegex {
        &self.imp().host_regex
    }

    pub fn flag(&self) -> u32 {
        self.imp().flag.get()
    }

    pub fn selected_text(&self) -> Option<String> {
        self.imp().selection.selected_item().and_downcast::<Item>().and_then(|i| i.string(COL_NAME))
    }

    fn selection_changed(&self) {
        let imp = self.imp();
        // The selected tracker or directory went away, show all torrents
        let Some(item) = imp.selection.selected_item().and_downcast::<Item>() else {
            imp.selection.set_selected(0);
            return;
        };
        imp.flag.set(item.uint(COL_BIT));
        self.prefs().set_int(
            prefs::STATE_SELECTOR_LAST,
            item.uint(COL_INDEX) as i64,
            prefs::GLOBAL,
        );
        self.emit_by_name::<()>("torrent-state-changed", &[&imp.flag.get()]);
    }

    fn remove_item(&self, item: &Item) {
        if let Some(pos) = self.imp().store.find(item) {
            self.imp().store.remove(pos);
        }
    }

    /// Removes the trackers or directories, or those not seen in the update
    /// with serial keep_serial if it's given.
    fn remove_dynamic(&self, dirs: bool, keep_serial: Option<i64>) {
        let imp = self.imp();
        let table = if dirs { &imp.directories } else { &imp.trackers };
        let removed: Vec<Item> = {
            let mut t = table.borrow_mut();
            let keys: Vec<String> = t
                .iter()
                .filter(|(_, v)| keep_serial.is_none_or(|s| v.int64(COL_SERIAL) != s))
                .map(|(k, _)| k.clone())
                .collect();
            keys.into_iter().filter_map(|k| t.remove(&k)).collect()
        };
        for item in removed {
            self.remove_item(&item);
        }
    }

    fn update_dynamic_filter(item: &Item, serial: i64) {
        if item.int64(COL_SERIAL) != serial {
            item.set(COL_COUNT, 1i64);
            item.set(COL_SERIAL, serial);
        } else {
            item.set(COL_COUNT, item.int64(COL_COUNT) + 1);
        }
    }

    /// Inserts a row for a tracker or directory, in name order among the
    /// range rows after offset (all of them if range is None).
    fn insert(&self, offset: usize, range: Option<usize>, item: &Item) {
        let store = &self.imp().store;
        let name = item.string(COL_NAME);
        let n = store.n_items() as usize;
        let mut pos = offset.min(n);
        let end = range.map(|r| n.min(offset + r)).unwrap_or(n);
        while pos < end {
            let other = store.item(pos as u32).and_downcast::<Item>().unwrap();
            if other.string(COL_NAME) >= name {
                break;
            }
            pos += 1;
        }
        store.insert(pos as u32, item);
    }

    fn filter_item_new(icon: &str, name: &str, bit: u32, serial: i64) -> Item {
        let item = new_state_item(Some(icon), Some(name), bit, 0);
        item.set(COL_SERIAL, serial);
        item.set(COL_COUNT, 1i64);
        item
    }

    /// Adds, counts and removes the tracker and/or directory rows
    fn update_filters(&self, update_trackers: bool, update_dirs: bool) {
        let client = self.client();
        let imp = self.imp();
        let serial = client.serial();

        if !client.is_connected() || !(update_trackers || update_dirs) {
            return;
        }
        let Some(model) = client.torrent_model() else { return };
        let n_cat = imp.n_categories.get() as usize;

        for torrent in item::items(&model.store()) {
            let Some(t) = torrent.torrent_json() else { continue };

            if update_trackers {
                for tracker in p::objects(p::torrent_get_tracker_stats(&t)) {
                    let Some(host) =
                        get_str(tracker, p::FIELD_ANNOUNCE).and_then(|a| imp.host_regex.first(a))
                    else {
                        continue;
                    };
                    let existing = imp.trackers.borrow().get(&host).cloned();
                    if let Some(item) = existing {
                        Self::update_dynamic_filter(&item, serial);
                        continue;
                    }
                    let item = Self::filter_item_new(
                        "network-workgroup",
                        &host,
                        FILTER_FLAG_TRACKER,
                        serial,
                    );
                    if imp.dirs_first.get() {
                        self.insert(n_cat + imp.directories.borrow().len(), None, &item);
                    } else {
                        self.insert(n_cat, Some(imp.trackers.borrow().len()), &item);
                    }
                    imp.trackers.borrow_mut().insert(host, item);
                }
            }

            if update_dirs {
                let Some(dir) = torrent.string(tm::COL_DOWNLOADDIR_SHORT) else { continue };
                let existing = imp.directories.borrow().get(&dir).cloned();
                if let Some(item) = existing {
                    Self::update_dynamic_filter(&item, serial);
                    continue;
                }
                let item = Self::filter_item_new("folder", &dir, FILTER_FLAG_DIR, serial);
                if imp.dirs_first.get() {
                    self.insert(n_cat, Some(imp.directories.borrow().len()), &item);
                } else {
                    self.insert(n_cat + imp.trackers.borrow().len(), None, &item);
                }
                imp.directories.borrow_mut().insert(dir, item);
            }
        }

        if update_trackers {
            self.remove_dynamic(false, Some(serial));
        }
        if update_dirs {
            self.remove_dynamic(true, Some(serial));
        }
    }

    pub fn update(&self, changed: u32) {
        let imp = self.imp();
        self.update_filters(
            imp.show_trackers.get() && changed & tm::UPDATE_ADDREMOVE != 0,
            imp.show_dirs.get() && changed & (tm::UPDATE_ADDREMOVE | tm::UPDATE_PATH_CHANGE) != 0,
        );
    }

    pub fn set_show_dirs(&self, show: bool) {
        self.imp().show_dirs.set(show);
        // rebuilt, the rows count each torrent once per update
        self.remove_dynamic(true, None);
        if show {
            self.update_filters(false, true);
        }
    }

    pub fn set_show_trackers(&self, show: bool) {
        self.imp().show_trackers.set(show);
        self.remove_dynamic(false, None);
        if show {
            self.update_filters(true, false);
        }
    }

    pub fn set_directories_first(&self, first: bool) {
        let imp = self.imp();
        imp.dirs_first.set(first);
        self.remove_dynamic(true, None);
        self.remove_dynamic(false, None);
        self.update_filters(imp.show_trackers.get(), imp.show_dirs.get());
    }

    /// Adds a state row at pos (or at the end); its index (for restoring the
    /// selection) is the number of rows before it was added.
    fn add_state(
        &self,
        pos: Option<u32>,
        icon: Option<&str>,
        name: Option<&str>,
        flag: u32,
    ) -> Item {
        let imp = self.imp();
        let item = new_state_item(icon, name, flag, imp.store.n_items());
        match pos {
            Some(pos) => imp.store.insert(pos, &item),
            None => imp.store.append(&item),
        }
        imp.n_categories.set(imp.n_categories.get() + 1);
        item
    }

    fn remove_state(&self, item: Option<Item>) {
        if let Some(item) = item {
            self.remove_item(&item);
            let n = &self.imp().n_categories;
            n.set(n.get() - 1);
        }
    }

    fn set_count(item: &Option<Item>, count: i64) {
        if let Some(item) = item {
            item.set(COL_COUNT, count);
        }
    }

    pub fn stats_update(&self, stats: &Stats) {
        let imp = self.imp();
        let has_error = imp.states.borrow().error.is_some();
        if stats.error > 0 && !has_error {
            let pos = (imp.n_categories.get() - 1).max(0) as u32;
            let item = self.add_state(
                Some(pos),
                Some("dialog-warning"),
                Some(&tr("Error")),
                p::TORRENT_FLAG_ERROR,
            );
            imp.states.borrow_mut().error = Some(item);
        } else if stats.error < 1 && has_error {
            let item = imp.states.borrow_mut().error.take();
            self.remove_state(item);
        }

        let s = imp.states.borrow();
        Self::set_count(&s.all, stats.count as i64);
        Self::set_count(&s.down, stats.down as i64);
        Self::set_count(&s.seeding, stats.seeding as i64);
        Self::set_count(&s.error, stats.error as i64);
        Self::set_count(&s.paused, stats.paused as i64);
        Self::set_count(&s.complete, stats.complete as i64);
        Self::set_count(&s.incomplete, stats.incomplete as i64);
        Self::set_count(&s.active, stats.active as i64);
        Self::set_count(&s.checking, stats.checking as i64);
        Self::set_count(&s.down_wait, stats.down_wait as i64);
        Self::set_count(&s.seed_wait, stats.seed_wait as i64);
    }

    pub fn disconnect(&self) {
        let imp = self.imp();
        let error = imp.states.borrow_mut().error.take();
        self.remove_state(error);
        self.remove_dynamic(false, None);
        self.remove_dynamic(true, None);

        let s = imp.states.borrow();
        for item in [
            &s.all,
            &s.down,
            &s.seeding,
            &s.paused,
            &s.complete,
            &s.incomplete,
            &s.active,
            &s.checking,
            &s.down_wait,
            &s.seed_wait,
        ] {
            Self::set_count(item, -1);
        }
    }

    pub fn set_queues_enabled(&self, enabled: bool) {
        let imp = self.imp();
        if enabled {
            let down = self.add_state(
                Some(2),
                Some("media-seek-backward"),
                Some(&tr("Queue Down")),
                p::TORRENT_FLAG_DOWNLOADING_WAIT,
            );
            let up = self.add_state(
                Some(4),
                Some("media-seek-forward"),
                Some(&tr("Queue Up")),
                p::TORRENT_FLAG_SEEDING_WAIT,
            );
            // their indices when they were added at the start
            down.set(COL_INDEX, 2u32);
            up.set(COL_INDEX, 4u32);
            let mut s = imp.states.borrow_mut();
            s.down_wait = Some(down);
            s.seed_wait = Some(up);
        } else {
            let (up, down) = {
                let mut s = imp.states.borrow_mut();
                (s.seed_wait.take(), s.down_wait.take())
            };
            self.remove_state(up);
            self.remove_state(down);
        }
    }

    /// Selects the row at position, and shows it
    fn select(&self, position: u32) {
        let imp = self.imp();
        imp.selection.set_selected(position);
        if let Some(lv) = imp.list_view.borrow().as_ref() {
            let _ = lv.activate_action("list.scroll-to-item", Some(&position.to_variant()));
        }
    }

    /// Type ahead search: typing selects the first row whose name starts
    /// with the typed text.
    fn key_pressed(&self, keyval: gdk::Key, state: gdk::ModifierType) -> bool {
        let imp = self.imp();
        let Some(c) = keyval.to_unicode() else { return false };
        if c.is_whitespace()
            || c.is_control()
            || state.intersects(
                gdk::ModifierType::CONTROL_MASK
                    | gdk::ModifierType::ALT_MASK
                    | gdk::ModifierType::META_MASK
                    | gdk::ModifierType::SUPER_MASK,
            )
        {
            return false;
        }
        let now = glib::monotonic_time();
        if now - imp.search_time.get() > 1_000_000 {
            imp.search.borrow_mut().clear();
        }
        imp.search_time.set(now);
        imp.search.borrow_mut().push(c);
        let needle = glib::casefold(imp.search.borrow().as_str()).to_string();

        let found = item::items(&imp.store).position(|item| {
            item.string(COL_NAME).is_some_and(|n| glib::casefold(&n).starts_with(&needle))
        });
        if let Some(i) = found {
            self.select(i as u32);
        }
        true
    }

    /// The list item of a row widget (a child of the list view)
    fn row_list_item(row: &gtk::Widget) -> Option<gtk::ListItem> {
        let mut child = row.first_child();
        while let Some(c) = child {
            if let Some(li) = unsafe { c.data::<glib::WeakRef<gtk::ListItem>>("trg-list-item") }
                .and_then(|p| unsafe { p.as_ref() }.upgrade())
            {
                return Some(li);
            }
            child = c.next_sibling();
        }
        None
    }

    /// The position of the row with the focus
    fn focus_position(&self) -> Option<u32> {
        let lv = self.imp().list_view.borrow().clone()?;
        let mut w = lv.root()?.focus()?;
        if !w.is_ancestor(&lv) {
            return None;
        }
        // up to the row widget, a child of the list view
        while w.parent().as_ref() != Some(lv.upcast_ref()) {
            w = w.parent()?;
        }
        Self::row_list_item(&w).map(|li| li.position())
    }

    /// The row widget that shows position
    fn row_widget(&self, position: u32) -> Option<gtk::Widget> {
        let lv = self.imp().list_view.borrow().clone()?;
        let mut child = lv.first_child();
        while let Some(c) = child {
            if Self::row_list_item(&c).is_some_and(|li| li.position() == position) {
                return Some(c);
            }
            child = c.next_sibling();
        }
        None
    }

    /// Moving the cursor up or down onto the separator moves it past it
    fn skip_separator(&self, keyval: gdk::Key, state: gdk::ModifierType) -> bool {
        let step: i64 = match keyval {
            gdk::Key::Up | gdk::Key::KP_Up => -1,
            gdk::Key::Down | gdk::Key::KP_Down => 1,
            _ => return false,
        };
        // not Caps Lock, which isn't a modifier here
        if state.intersects(gtk::accelerator_get_default_mod_mask()) {
            return false;
        }
        let store = &self.imp().store;
        let n = store.n_items() as i64;
        let Some(pos) = self.focus_position().map(|p| p as i64) else { return false };
        if (step < 0 && pos < 2) || pos + 2 * step >= n {
            return false;
        }
        let item = store.item((pos + step) as u32).and_downcast::<Item>().unwrap();
        if item.string(COL_NAME).is_some() {
            return false;
        }
        let target = (pos + 2 * step) as u32;
        // The focus moves along, or the next key press would start above the
        // separator again and land on the same row
        if let Some(row) = self.row_widget(target) {
            row.grab_focus();
        }
        self.select(target);
        true
    }

    fn context_menu(&self, mut x: f64, mut y: f64) -> bool {
        // From the keyboard, point at the row with the focus
        if (x < 0.0 || y < 0.0)
            && let Some(focus) = self.root().and_then(|r| r.focus()).filter(|f| f.is_ancestor(self))
            && let Some(b) = focus.compute_bounds(self)
        {
            x = b.x() as f64 + 20.0;
            y = b.y() as f64 + b.height() as f64 / 2.0;
        }

        let actions = gio::SimpleActionGroup::new();
        let action = gio::SimpleAction::new("refresh", None);
        let weak = self.downgrade();
        action.connect_activate(move |_, _| {
            if let Some(s) = weak.upgrade() {
                s.client().inc_serial();
                s.update(tm::UPDATE_ADDREMOVE);
            }
        });
        actions.add_action(&action);

        let menu = gio::Menu::new();
        let item = gio::MenuItem::new(Some(&tr("Refresh")), Some("state.refresh"));
        item.set_icon(&gio::ThemedIcon::new("view-refresh"));
        menu.append_item(&item);

        util::popover_menu_popup(self, &menu, x, y, "state", Some(&actions));
        true
    }

    fn build(&self) {
        let imp = self.imp();
        let prefs = self.prefs();

        imp.show_dirs.set(prefs.get_bool(prefs::KEY_FILTER_DIRS, prefs::GLOBAL));
        imp.show_trackers.set(prefs.get_bool(prefs::KEY_FILTER_TRACKERS, prefs::GLOBAL));
        imp.dirs_first.set(prefs.get_bool(prefs::KEY_DIRECTORIES_FIRST, prefs::GLOBAL));

        let f = &p::TORRENT_FLAG_DOWNLOADING;
        let s = States {
            all: Some(self.add_state(None, Some("help-about"), Some(&tr("All")), 0)),
            down: Some(self.add_state(None, Some("go-down"), Some(&tr("Downloading")), *f)),
            down_wait: Some(self.add_state(
                None,
                Some("media-seek-backward"),
                Some(&tr("Queue Down")),
                p::TORRENT_FLAG_DOWNLOADING_WAIT,
            )),
            seeding: Some(self.add_state(
                None,
                Some("go-up"),
                Some(&tr("Seeding")),
                p::TORRENT_FLAG_SEEDING,
            )),
            seed_wait: Some(self.add_state(
                None,
                Some("media-seek-forward"),
                Some(&tr("Queue Up")),
                p::TORRENT_FLAG_SEEDING_WAIT,
            )),
            paused: Some(self.add_state(
                None,
                Some("media-playback-pause"),
                Some(&tr("Paused")),
                p::TORRENT_FLAG_PAUSED,
            )),
            complete: Some(self.add_state(
                None,
                Some("trg-gtk-apply"),
                Some(&tr("Complete")),
                p::TORRENT_FLAG_COMPLETE,
            )),
            incomplete: Some(self.add_state(
                None,
                Some("edit-select-all"),
                Some(&tr("Incomplete")),
                p::TORRENT_FLAG_INCOMPLETE,
            )),
            active: Some(self.add_state(
                None,
                Some("network-workgroup"),
                Some(&tr("Active")),
                p::TORRENT_FLAG_ACTIVE,
            )),
            checking: Some(self.add_state(
                None,
                Some("view-refresh"),
                Some(&tr("Checking")),
                p::TORRENT_FLAG_CHECKING_ANY,
            )),
            ..Default::default()
        };
        self.add_state(None, None, None, 0);
        *imp.states.borrow_mut() = s;

        // Always one filter selected, "All" by default
        imp.selection.set_autoselect(false);
        imp.selection.set_can_unselect(false);
        let weak = self.downgrade();
        imp.selection.connect_selected_item_notify(move |_| {
            if let Some(s) = weak.upgrade() {
                s.selection_changed();
            }
        });

        let mut index = prefs.get_int(prefs::STATE_SELECTOR_LAST, prefs::GLOBAL);
        if index < 0 || index >= (imp.n_categories.get() - 1) as i64 {
            index = 0;
        }
        imp.selection.set_selected(index as u32);
        if index == 0 {
            // "selected-item" doesn't change from the initial selection
            self.selection_changed();
        }

        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, obj| {
            let Some(li) = obj.downcast_ref::<gtk::ListItem>() else { return };
            let bx = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            bx.append(&gtk::Image::new());
            let label = gtk::Label::new(None);
            label.set_xalign(0.0);
            label.set_hexpand(true);
            bx.append(&label);
            let count = gtk::Label::new(None);
            count.add_css_class("dim-label");
            count.add_css_class("numeric");
            // room for two digits, so the sidebar doesn't widen as they come
            count.set_width_chars(2);
            count.set_xalign(1.0);
            bx.append(&count);
            let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
            sep.set_hexpand(true);
            sep.set_valign(gtk::Align::Center);
            bx.append(&sep);
            tree_view::mark_row_widget(&bx, li);
            li.set_child(Some(&bx));
        });
        factory.connect_bind(|_, obj| {
            let Some(li) = obj.downcast_ref::<gtk::ListItem>() else { return };
            let Some(item) = li.item().and_downcast::<Item>() else { return };
            let separator = item.string(COL_NAME).is_none();
            // the separator row (between the states and the other filters)
            // shows only its separator, and can't be selected
            let bx = li.child().unwrap();
            let mut child = bx.first_child();
            while let Some(c) = child {
                c.set_visible(c.is::<gtk::Separator>() == separator);
                child = c.next_sibling();
            }
            li.set_selectable(!separator);
            li.set_activatable(!separator);
            set_row_separator(&bx, separator);

            // The trackers' and directories' names can be long (paths). They
            // don't ask for any room, so only the states set the sidebar's
            // width, and are ellipsized: hosts at the end, paths at the start,
            // which keeps their last directories. The tooltip has all of it.
            let bit = item.uint(COL_BIT);
            let dynamic = bit == FILTER_FLAG_DIR || bit == FILTER_FLAG_TRACKER;
            bx.set_tooltip_text(item.string(COL_NAME).as_deref().filter(|_| dynamic));
            if separator {
                return;
            }
            let name = bx.first_child().and_then(|c| c.next_sibling()).and_downcast::<gtk::Label>();
            if let Some(name) = name {
                name.set_ellipsize(match bit {
                    FILTER_FLAG_DIR => gtk::pango::EllipsizeMode::Start,
                    FILTER_FLAG_TRACKER => gtk::pango::EllipsizeMode::End,
                    _ => gtk::pango::EllipsizeMode::None,
                });
                name.set_max_width_chars(if dynamic { 0 } else { -1 });
            }
            let weak = li.downgrade();
            let id = item.connect_local("column-changed", false, move |_| {
                if let Some(li) = weak.upgrade() {
                    row_refresh(&li);
                }
                None
            });
            unsafe {
                li.set_data("trg-handler", id);
            }
            row_refresh(li);
        });
        factory.connect_unbind(|_, obj| {
            let Some(li) = obj.downcast_ref::<gtk::ListItem>() else { return };
            if let (Some(item), Some(id)) =
                (li.item(), unsafe { li.steal_data::<glib::SignalHandlerId>("trg-handler") })
            {
                item.disconnect(id);
            }
        });

        let lv = gtk::ListView::new(Some(imp.selection.clone()), Some(factory));
        lv.add_css_class("navigation-sidebar");

        let keys = gtk::EventControllerKey::new();
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, keyval, _, state| match weak.upgrade() {
            Some(s) if s.key_pressed(keyval, state) => glib::Propagation::Stop,
            _ => glib::Propagation::Proceed,
        });
        lv.add_controller(keys);

        // before the list view's own cursor keys
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, keyval, _, state| match weak.upgrade() {
            Some(s) if s.skip_separator(keyval, state) => glib::Propagation::Stop,
            _ => glib::Propagation::Proceed,
        });
        lv.add_controller(keys);

        // The separator row is a line with some room, without a row's height and hover
        let css = gtk::CssProvider::new();
        css.load_from_data(
            ".navigation-sidebar > row.trg-separator { \
                min-height: 0; padding-top: 4px; padding-bottom: 4px; background: none; }",
        );
        gtk::style_context_add_provider_for_display(
            &self.display(),
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let scrolled = util::scrolled_window(&lv);
        // the sidebar is as wide as the states (at least), see the bind above
        scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scrolled.set_parent(self);
        *imp.scrolled.borrow_mut() = Some(scrolled);
        *imp.list_view.borrow_mut() = Some(lv);

        let weak = self.downgrade();
        util::add_context_menu(self, move |_, x, y| {
            weak.upgrade().is_some_and(|s| s.context_menu(x, y))
        });
    }
}

/// Makes the row widget of a list item's child (its parent) the separator, or
/// not: it can't take the focus (from a click or the keyboard; the list item's
/// "focusable" needs GTK 4.12), and isn't styled like a row (see build).
fn set_row_separator(child: &gtk::Widget, separator: bool) {
    use glib::translate::{IntoGlib, ToGlibPtr};
    // Only raw calls: while a new row is set up and bound it's still
    // floating, and a wrapper of it would sink that reference and free it.
    unsafe {
        let row = gtk::ffi::gtk_widget_get_parent(child.to_glib_none().0);
        if row.is_null() {
            return;
        }
        gtk::ffi::gtk_widget_set_focusable(row, (!separator).into_glib());
        let class = c"trg-separator".as_ptr();
        if separator {
            gtk::ffi::gtk_widget_add_css_class(row, class);
        } else {
            gtk::ffi::gtk_widget_remove_css_class(row, class);
        }
    }
}

/// The rows: an icon, the name and the count, or a separator
fn row_refresh(li: &gtk::ListItem) {
    let (Some(item), Some(bx)) = (li.item().and_downcast::<Item>(), li.child()) else {
        return;
    };
    let image = bx.first_child().and_downcast::<gtk::Image>().unwrap();
    let name = image.next_sibling().and_downcast::<gtk::Label>().unwrap();
    let count_label = name.next_sibling().and_downcast::<gtk::Label>().unwrap();
    let count = item.int64(COL_COUNT);

    image.set_icon_name(item.string(COL_ICON).as_deref());
    name.set_text(&item.string(COL_NAME).unwrap_or_default());
    if count > 0 {
        count_label.set_text(&count.to_string());
    }
    count_label.set_visible(count > 0);
}
