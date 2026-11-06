//! The trackers of the selected torrent (trg-trackers-model.c and
//! trg-trackers-tree-view.c): editing an announce URL replaces the tracker,
//! the context menu adds and deletes them. Changes send the whole list of
//! the rows (tracker_list).

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};

use crate::client::Client;
use crate::i18n::tr;
use crate::item::{self, Item, Val};
use crate::main_window::MainWindow;
use crate::protocol::{self as p, Obj, get_i64, get_str};
use crate::tree_view::{
    COLUMN_EDITABLE, COLUMN_EXTRA, COLUMN_UNREMOVABLE, ColType, ColumnDesc, TreeView,
};
use crate::util;

pub const COL_ICON: usize = 0;
pub const COL_TIER: usize = 1;
pub const COL_ANNOUNCE: usize = 2;
pub const COL_SCRAPE: usize = 3;
pub const COL_ID: usize = 4;
pub const COL_LAST_ANNOUNCE_PEER_COUNT: usize = 5;
pub const COL_LAST_ANNOUNCE_TIME: usize = 6;
pub const COL_LAST_SCRAPE_TIME: usize = 7;
pub const COL_SEEDERCOUNT: usize = 8;
pub const COL_LEECHERCOUNT: usize = 9;
pub const COL_HOST: usize = 10;
pub const COL_LAST_ANNOUNCE_RESULT: usize = 11;
pub const COL_UPDATESERIAL: usize = 12;
pub const COL_DOWNLOADERCOUNT: usize = 13;

fn new_tracker_item() -> Item {
    Item::new(vec![
        Val::Str(None),
        Val::I64(0),
        Val::Str(None),
        Val::Str(None),
        Val::I64(0),
        Val::I64(0),
        Val::I64(0),
        Val::I64(0),
        Val::I64(0),
        Val::I64(0),
        Val::Str(None),
        Val::Str(None),
        Val::I64(0),
        Val::I64(0),
    ])
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
            store: gio::ListStore::new::<Item>(),
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

    /// A new row for a tracker to add, which the user edits the announce URL of
    pub fn add_new(&self) -> Item {
        let tier = item::items(&self.store).map(|i| i.int64(COL_TIER) + 1).max().unwrap_or(0);
        let item = new_tracker_item();
        item.set(COL_ICON, "list-add");
        item.set(COL_ID, -1i64);
        item.set(COL_TIER, tier);
        self.store.append(&item);
        item
    }

    pub fn update(&self, serial: i64, t: &Obj, mode: i32) {
        if mode == p::TORRENT_GET_MODE_FIRST {
            self.clear();
            self.torrent_id.set(p::torrent_get_id(t));
            self.accept.set(true);
        } else if !self.accept.get() {
            return;
        }

        for tracker in p::objects(p::torrent_get_tracker_stats(t)) {
            let id = get_i64(tracker, p::FIELD_ID);
            let existing = if mode != p::TORRENT_GET_MODE_FIRST {
                item::find_item(&self.store, COL_ID, id).map(|(i, _)| i)
            } else {
                None
            };
            let item = existing.unwrap_or_else(|| {
                let item = new_tracker_item();
                self.store.append(&item);
                item
            });

            item.set(COL_ICON, "network-workgroup");
            item.set(COL_ID, id);
            item.set(COL_UPDATESERIAL, serial);
            item.set(COL_TIER, get_i64(tracker, p::FIELD_TIER));
            item.set(COL_ANNOUNCE, get_str(tracker, p::FIELD_ANNOUNCE));
            item.set(COL_SCRAPE, get_str(tracker, p::FIELD_SCRAPE));
            item.set(COL_HOST, get_str(tracker, p::FIELD_HOST));
            item.set(COL_LAST_ANNOUNCE_RESULT, get_str(tracker, p::FIELD_LAST_ANNOUNCE_RESULT));
            item.set(COL_LAST_ANNOUNCE_TIME, get_i64(tracker, p::FIELD_LAST_ANNOUNCE_TIME));
            item.set(COL_LAST_SCRAPE_TIME, get_i64(tracker, p::FIELD_LAST_SCRAPE_TIME));
            item.set(
                COL_LAST_ANNOUNCE_PEER_COUNT,
                get_i64(tracker, p::FIELD_LAST_ANNOUNCE_PEER_COUNT),
            );
            item.set(COL_LEECHERCOUNT, get_i64(tracker, p::FIELD_LEECHERCOUNT));
            item.set(COL_SEEDERCOUNT, get_i64(tracker, p::FIELD_SEEDERCOUNT));
            item.set(COL_DOWNLOADERCOUNT, get_i64(tracker, p::FIELD_DOWNLOADER_COUNT));
        }

        item::remove_removed(&self.store, COL_UPDATESERIAL, serial);
    }
}

/// The trackers view
pub struct TrackersView {
    pub tv: TreeView,
    model: Rc<TrackersModel>,
    win: glib::WeakRef<MainWindow>,
    announce_column: Rc<ColumnDesc>,
}

impl TrackersView {
    pub fn new(
        model: &Rc<TrackersModel>,
        client: &Client,
        win: &MainWindow,
        config_id: Option<&str>,
    ) -> Rc<Self> {
        let tv = TreeView::new(Some(client.prefs()), config_id.unwrap_or("TrgTrackersTreeView"));

        let desc = tv.reg_column(
            ColType::IconText,
            COL_TIER as i32,
            &tr("Tier"),
            "tier",
            COLUMN_UNREMOVABLE,
        );
        desc.set_extra(COL_ICON as i32);

        let announce = tv.reg_column(
            ColType::Text,
            COL_ANNOUNCE as i32,
            &tr("Announce URL"),
            "announce-url",
            COLUMN_UNREMOVABLE | COLUMN_EDITABLE,
        );
        announce.set_editable(true);

        let reg = |t, c: usize, h: &str, id: &str, f| {
            tv.reg_column(t, c as i32, h, id, f);
        };
        reg(
            ColType::NumGtZero,
            COL_LAST_ANNOUNCE_PEER_COUNT,
            &tr("Peers"),
            "last-announce-peer-count",
            0,
        );
        reg(ColType::NumGtZero, COL_SEEDERCOUNT, &tr("Seeder Count"), "seeder-count", 0);
        reg(ColType::NumGtZero, COL_LEECHERCOUNT, &tr("Leecher Count"), "leecher-count", 0);
        reg(
            ColType::NumGtZero,
            COL_DOWNLOADERCOUNT,
            &tr("Downloader Count"),
            "downloader-count",
            COLUMN_EXTRA,
        );
        reg(ColType::Epoch, COL_LAST_ANNOUNCE_TIME, &tr("Last Announce"), "last-announce-time", 0);
        reg(ColType::Text, COL_LAST_ANNOUNCE_RESULT, &tr("Last Result"), "last-result", 0);
        reg(ColType::Text, COL_SCRAPE, &tr("Scrape URL"), "scrape-url", 0);
        reg(
            ColType::Epoch,
            COL_LAST_SCRAPE_TIME,
            &tr("Last Scrape"),
            "last-scrape-time",
            COLUMN_EXTRA,
        );
        reg(ColType::Text, COL_HOST, &tr("Host"), "host", COLUMN_EXTRA);

        let view = Rc::new(Self {
            tv: tv.clone(),
            model: model.clone(),
            win: win.downgrade(),
            announce_column: announce.clone(),
        });

        let weak = Rc::downgrade(&view);
        announce.set_edit(move |_, item, done, new_text| {
            if let Some(view) = weak.upgrade() {
                view.announce_edit(item, done, new_text);
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
        let Some(win) = self.win.upgrade() else { return };
        let trackers: Vec<(i64, String)> = item::items(&self.model.store)
            .filter_map(|i| Some((i.int64(COL_TIER), i.string(COL_ANNOUNCE)?)))
            .filter(|(_, url)| !url.is_empty())
            .collect();
        let req = p::torrent_set(&[self.model.torrent_id()])
            .with(p::FIELD_TRACKER_LIST, p::tracker_list(&trackers));
        self.model.set_accept(false);
        let model = self.model.clone();
        win.dispatch_interactive_with(req, move |_| model.set_accept(true));
    }

    /// Editing an announce URL replaces the tracker, or adds it for a new row
    fn announce_edit(self: &Rc<Self>, item: &Item, done: bool, new_text: Option<&str>) {
        if !done {
            self.model.set_accept(false);
            return;
        }
        let Some(new_text) = new_text.filter(|t| !t.is_empty()) else {
            self.model.set_accept(true);
            return;
        };

        item.set(COL_ANNOUNCE, new_text);
        self.send_tracker_list();
    }

    fn add_tracker(&self) {
        let item = self.model.add_new();
        self.tv.select_item(&item);
        self.tv.start_editing(&item, &self.announce_column);
    }

    fn delete_tracker(self: &Rc<Self>) {
        for item in self.tv.selected_items() {
            if let Some(pos) = self.model.store.find(&item) {
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
            "/io/github/TransmissionRemoteGtk/ui/trg-trackers-tree-view.ui",
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
