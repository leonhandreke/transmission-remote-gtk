//! The trackers of the selected torrent: editing an announce URL replaces the
//! tracker, the context menu adds and deletes them. Changes send the whole list
//! of the rows (tracker_list).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

use crate::client::Client;
use crate::i18n::tr;
use crate::protocol::{self as p, Torrent};
use crate::torrent_model::UpdateMode;
use crate::tree_view::{
    COLUMN_EDITABLE, COLUMN_EXTRA, COLUMN_UNREMOVABLE, ColType, ColumnDesc, TreeView,
};
use crate::util::{self, set_changed};

/* A tracker of the torrent, or a new one being added */

mod imp {
    use super::*;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::TrackerObject)]
    pub struct TrackerObject {
        #[property(get, set)]
        pub icon: RefCell<String>,
        /// The server's ID of the tracker, -1 for a new one
        #[property(get, set)]
        pub id: Cell<i64>,
        #[property(get, set)]
        pub tier: Cell<i64>,
        #[property(get, set)]
        pub announce: RefCell<String>,
        #[property(get, set)]
        pub scrape: RefCell<String>,
        #[property(get, set)]
        pub host: RefCell<String>,
        #[property(get, set)]
        pub last_announce_result: RefCell<String>,
        #[property(get, set)]
        pub last_announce_time: Cell<i64>,
        #[property(get, set)]
        pub last_scrape_time: Cell<i64>,
        #[property(get, set)]
        pub last_announce_peer_count: Cell<i64>,
        #[property(get, set)]
        pub seeder_count: Cell<i64>,
        #[property(get, set)]
        pub leecher_count: Cell<i64>,
        #[property(get, set)]
        pub downloader_count: Cell<i64>,
        /// The serial of the update it was last in
        #[property(get, set)]
        pub serial: Cell<i64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TrackerObject {
        const NAME: &'static str = "TrgTrackerObject";
        type Type = super::TrackerObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for TrackerObject {}
}

glib::wrapper! {
    pub struct TrackerObject(ObjectSubclass<imp::TrackerObject>);
}

impl TrackerObject {
    fn new() -> Self {
        glib::Object::new()
    }

    fn update(&self, serial: i64, tracker: &p::TrackerStat) {
        set_changed(self, "icon", "network-workgroup".to_string());
        set_changed(self, "id", tracker.id);
        self.set_serial(serial);
        set_changed(self, "tier", tracker.tier);
        set_changed(self, "announce", tracker.announce.clone());
        set_changed(self, "scrape", tracker.scrape.clone());
        set_changed(self, "host", tracker.host.clone());
        set_changed(self, "last-announce-result", tracker.last_announce_result.clone());
        set_changed(self, "last-announce-time", tracker.last_announce_time);
        set_changed(self, "last-scrape-time", tracker.last_scrape_time);
        set_changed(self, "last-announce-peer-count", tracker.last_announce_peer_count);
        set_changed(self, "leecher-count", tracker.leecher_count);
        set_changed(self, "seeder-count", tracker.seeder_count);
        set_changed(self, "downloader-count", tracker.downloader_count);
    }
}

/// The trackers of a torrent
pub struct TrackersModel {
    pub store: gio::ListStore,
    torrent_id: Cell<i64>,
    accept: Cell<bool>,
}

impl TrackersModel {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            store: gio::ListStore::new::<TrackerObject>(),
            torrent_id: Cell::new(-1),
            accept: Cell::new(true),
        })
    }

    pub fn set_no_selection(&self) {
        self.torrent_id.set(-1);
    }

    pub fn torrent_id(&self) -> i64 {
        self.torrent_id.get()
    }

    pub fn clear(&self) {
        self.store.remove_all();
    }

    pub fn set_accept(&self, accept: bool) {
        self.accept.set(accept);
    }

    fn trackers(&self) -> impl Iterator<Item = TrackerObject> + '_ {
        util::objects(&self.store)
    }

    /// A new row for a tracker to add, which the user edits the announce URL of
    pub fn add_new(&self) -> TrackerObject {
        let tier = self.trackers().map(|t| t.tier() + 1).max().unwrap_or(0);
        let tracker = TrackerObject::new();
        tracker.set_icon("list-add");
        tracker.set_id(-1);
        tracker.set_tier(tier);
        self.store.append(&tracker);
        tracker
    }

    pub fn update(&self, serial: i64, t: &Torrent, mode: UpdateMode) {
        if mode == UpdateMode::First {
            self.clear();
            self.torrent_id.set(t.id);
            self.accept.set(true);
        } else if !self.accept.get() {
            return;
        }

        for tracker in &t.tracker_stats {
            let existing = if mode != UpdateMode::First {
                self.trackers().find(|o| o.id() == tracker.id)
            } else {
                None
            };
            let obj = existing.unwrap_or_else(|| {
                let obj = TrackerObject::new();
                self.store.append(&obj);
                obj
            });
            obj.update(serial, tracker);
        }

        util::remove_where(&self.store, |o: &TrackerObject| o.serial() != serial);
    }
}

/// The trackers view
pub struct TrackersView {
    pub tv: TreeView,
    model: Rc<TrackersModel>,
    client: Client,
    announce_column: Rc<ColumnDesc>,
}

impl TrackersView {
    pub fn new(model: &Rc<TrackersModel>, client: &Client, config_id: Option<&str>) -> Rc<Self> {
        let tv = TreeView::new(Some(client.prefs()), config_id.unwrap_or("TrgTrackersTreeView"));

        // the keys are the model columns the trackers had as Items
        let reg = |t, key, p, h: &str, id: &str, f| tv.reg_property(t, key, p, h, id, f);
        reg(ColType::IconText, 1, "tier", &tr("Tier"), "tier", COLUMN_UNREMOVABLE)
            .set_extra("icon");
        let announce = reg(
            ColType::Text,
            2,
            "announce",
            &tr("Announce URL"),
            "announce-url",
            COLUMN_UNREMOVABLE | COLUMN_EDITABLE,
        );
        announce.set_editable(true);
        reg(
            ColType::NumGtZero,
            5,
            "last-announce-peer-count",
            &tr("Peers"),
            "last-announce-peer-count",
            0,
        );
        reg(ColType::NumGtZero, 8, "seeder-count", &tr("Seeder Count"), "seeder-count", 0);
        reg(ColType::NumGtZero, 9, "leecher-count", &tr("Leecher Count"), "leecher-count", 0);
        reg(
            ColType::NumGtZero,
            13,
            "downloader-count",
            &tr("Downloader Count"),
            "downloader-count",
            COLUMN_EXTRA,
        );
        reg(ColType::Epoch, 6, "last-announce-time", &tr("Last Announce"), "last-announce-time", 0);
        reg(ColType::Text, 11, "last-announce-result", &tr("Last Result"), "last-result", 0);
        reg(ColType::Text, 3, "scrape", &tr("Scrape URL"), "scrape-url", 0);
        reg(
            ColType::Epoch,
            7,
            "last-scrape-time",
            &tr("Last Scrape"),
            "last-scrape-time",
            COLUMN_EXTRA,
        );
        reg(ColType::Text, 10, "host", &tr("Host"), "host", COLUMN_EXTRA);

        let view = Rc::new(Self {
            tv: tv.clone(),
            model: model.clone(),
            client: client.clone(),
            announce_column: announce.clone(),
        });

        let weak = Rc::downgrade(&view);
        announce.set_edit(move |_, tracker: &TrackerObject, done, new_text| {
            if let Some(view) = weak.upgrade() {
                view.announce_edit(tracker, done, new_text);
            }
        });

        tv.set_model(&model.store);
        tv.setup_columns();
        tv.restore_sort();

        let weak = Rc::downgrade(&view);
        util::add_context_menu(&tv, move |_, x, y| {
            weak.upgrade().is_some_and(|v| v.context_menu(x, y))
        });

        view
    }

    /// Sends the announce URLs of the rows as the torrent's tracker list,
    /// ignoring updates from the server meanwhile
    fn send_tracker_list(self: &Rc<Self>) {
        let trackers: Vec<(i64, String)> = self
            .model
            .trackers()
            .map(|t| (t.tier(), t.announce()))
            .filter(|(_, url)| !url.is_empty())
            .collect();
        let req = p::torrent_set(&[self.model.torrent_id()])
            .with(p::FIELD_TRACKER_LIST, p::tracker_list(&trackers));
        self.model.set_accept(false);
        let model = self.model.clone();
        self.client.dispatch_interactive_with(req, move |_| model.set_accept(true));
    }

    /// Editing an announce URL replaces the tracker, or adds it for a new row
    fn announce_edit(self: &Rc<Self>, tracker: &TrackerObject, done: bool, new_text: Option<&str>) {
        if !done {
            self.model.set_accept(false);
            return;
        }
        let Some(new_text) = new_text.filter(|t| !t.is_empty()) else {
            self.model.set_accept(true);
            return;
        };

        set_changed(tracker, "announce", new_text.to_string());
        self.send_tracker_list();
    }

    fn add_tracker(&self) {
        let tracker = self.model.add_new();
        self.tv.select_item(&tracker);
        self.tv.start_editing(&tracker, &self.announce_column);
    }

    fn delete_tracker(self: &Rc<Self>) {
        for tracker in self.tv.selected_items::<TrackerObject>() {
            if let Some(pos) = self.model.store.find(&tracker) {
                self.model.store.remove(pos);
            }
        }
        self.send_tracker_list();
    }

    fn popup_menu(self: &Rc<Self>, x: f64, y: f64, with_delete: bool) {
        let actions = gio::SimpleActionGroup::new();
        let delete = gio::SimpleAction::new("delete", None);
        let weak = Rc::downgrade(self);
        delete.connect_activate(move |_, _| {
            if let Some(v) = weak.upgrade() {
                v.delete_tracker();
            }
        });
        actions.add_action(&delete);
        let add = gio::SimpleAction::new("add", None);
        let weak = Rc::downgrade(self);
        add.connect_activate(move |_, _| {
            if let Some(v) = weak.upgrade() {
                v.add_tracker();
            }
        });
        actions.add_action(&add);

        let builder = gtk::Builder::from_resource(
            "/io/github/TransmissionRemoteGtk/ui/trackers-tree-view.ui",
        );
        let menu = gio::Menu::new();
        if with_delete && let Some(m) = builder.object::<gio::MenuModel>("delete_menu") {
            menu.append_section(None, &m);
        }
        if let Some(m) = builder.object::<gio::MenuModel>("trackers_menu") {
            menu.append_section(None, &m);
        }

        util::popover_menu_popup(&self.tv, &menu, x, y, "trackers", Some(&actions));
    }

    fn context_menu(self: &Rc<Self>, x: f64, y: f64) -> bool {
        // from the keyboard
        if x < 0.0 {
            self.popup_menu(x, y, self.tv.count_selected() > 0);
            return true;
        }
        // the column headers have their own menu
        if self.tv.point_in_header(x, y) {
            return false;
        }
        if self.tv.select_at(x, y) {
            self.popup_menu(x, y, true);
        } else if self.model.torrent_id() >= 0 {
            self.popup_menu(x, y, false);
        }
        true
    }
}
