//! The torrents: a list of Items which updates from a torrent-get response.
//! It handles a number of different update modes:
//!   1) the first update,
//!   2) a full update,
//!   3) an active-only update,
//!   4) individual torrent updates.
//!
//! The items are updated in place (only the values that changed notify), and
//! looked up by ID in a hash table, so an update is linear in the number of
//! torrents.
//!
//! It also counts the torrents in each state and the total speeds, emits
//! signals when torrents are added, completed or changed state, shortens the
//! download directories (for the state selector) and the announce URLs.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::gio;
use gtk::glib;
use gtk::glib::subclass::prelude::*;
use gtk::prelude::*;

use crate::client::Client;
use crate::i18n::tr;
use crate::item::{self, Item, Val};
use crate::json::{Obj, get_str, objects};
use crate::prefs::{self, Scope};
use crate::protocol::{self as p, Torrent};
use crate::util;

pub const COL_ICON: usize = 0;
pub const COL_NAME: usize = 1;
pub const COL_SIZEWHENDONE: usize = 2;
pub const COL_PERCENTDONE: usize = 3;
pub const COL_METADATAPERCENTCOMPLETE: usize = 4;
pub const COL_STATUS: usize = 5;
pub const COL_SEEDS: usize = 6;
pub const COL_LEECHERS: usize = 7;
pub const COL_DOWNLOADS: usize = 8;
pub const COL_PEERS_CONNECTED: usize = 9;
pub const COL_PEERS_FROM_US: usize = 10;
pub const COL_WEB_SEEDS_TO_US: usize = 11;
pub const COL_PEERS_TO_US: usize = 12;
pub const COL_DOWNSPEED: usize = 13;
pub const COL_UPSPEED: usize = 14;
pub const COL_ETA: usize = 15;
pub const COL_UPLOADED: usize = 16;
pub const COL_DOWNLOADED: usize = 17;
pub const COL_TOTALSIZE: usize = 18;
pub const COL_HAVE_UNCHECKED: usize = 19;
pub const COL_HAVE_VALID: usize = 20;
pub const COL_RATIO: usize = 21;
pub const COL_ADDED: usize = 22;
pub const COL_ID: usize = 23;
pub const COL_TORRENT: usize = 24;
pub const COL_UPDATESERIAL: usize = 25;
pub const COL_FLAGS: usize = 26;
pub const COL_DOWNLOADDIR: usize = 27;
pub const COL_DOWNLOADDIR_SHORT: usize = 28;
pub const COL_BANDWIDTH_PRIORITY: usize = 29;
pub const COL_DONE_DATE: usize = 30;
pub const COL_FROMPEX: usize = 31;
pub const COL_FROMDHT: usize = 32;
pub const COL_FROMTRACKERS: usize = 33;
pub const COL_FROMLTEP: usize = 34;
pub const COL_FROMRESUME: usize = 35;
pub const COL_FROMINCOMING: usize = 36;
pub const COL_PEER_SOURCES: usize = 37;
pub const COL_TRACKERHOST: usize = 38;
pub const COL_QUEUE_POSITION: usize = 39;
pub const COL_LASTACTIVE: usize = 40;
pub const COL_FILECOUNT: usize = 41;
pub const COL_ERROR: usize = 42;
pub const COL_SEED_RATIO_MODE: usize = 43;
pub const COL_SEED_RATIO_LIMIT: usize = 44;
pub const N_COLUMNS: usize = 45;

/// What a torrent_get was for, which decides how it updates the models
#[derive(Clone, Copy, Debug, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "TrgUpdateMode")]
pub enum UpdateMode {
    /// All the torrents, after connecting (or the first look at a torrent):
    /// the models are filled from scratch
    First,
    /// The periodic update of the recently active torrents, with the IDs of
    /// the removed ones
    Active,
    /// Some torrents, after the user changed them
    Interaction,
    /// The periodic update of all the torrents: those missing were removed
    Update,
}

pub const UPDATE_STATE_CHANGE: u32 = 1 << 0;
pub const UPDATE_PATH_CHANGE: u32 = 1 << 1;
pub const UPDATE_ADDREMOVE: u32 = 1 << 2;

/// A torrent is an Item with the columns above. The torrent column holds the
/// torrent of the last update.
pub type TorrentItem = Item;

pub trait TorrentItemExt {
    fn torrent(&self) -> Option<Rc<Torrent>>;
    fn torrent_id(&self) -> i64;
}

impl TorrentItemExt for Item {
    fn torrent(&self) -> Option<Rc<Torrent>> {
        self.torrent_value(COL_TORRENT)
    }

    fn torrent_id(&self) -> i64 {
        self.int64(COL_ID)
    }
}

fn new_torrent_item() -> TorrentItem {
    let mut values = vec![Val::I64(0); N_COLUMNS];
    for c in [
        COL_ICON,
        COL_NAME,
        COL_STATUS,
        COL_DOWNLOADDIR,
        COL_DOWNLOADDIR_SHORT,
        COL_PEER_SOURCES,
        COL_TRACKERHOST,
    ] {
        values[c] = Val::Str(None);
    }
    for c in [COL_PERCENTDONE, COL_METADATAPERCENTCOMPLETE, COL_RATIO, COL_SEED_RATIO_LIMIT] {
        values[c] = Val::F64(0.0);
    }
    values[COL_FLAGS] = Val::U32(0);
    values[COL_FILECOUNT] = Val::U32(0);
    values[COL_TORRENT] = Val::Torrent(None);
    Item::new(values)
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Stats {
    pub down_rate_total: i64,
    pub up_rate_total: i64,
    pub seeding: i32,
    pub down: i32,
    pub paused: i32,
    pub count: i32,
    pub error: i32,
    pub complete: i32,
    pub incomplete: i32,
    pub checking: i32,
    pub active: i32,
    pub seed_wait: i32,
    pub down_wait: i32,
}

mod imp {
    use super::*;
    use std::sync::OnceLock;

    pub struct TorrentModel {
        pub store: gio::ListStore,
        pub ht: RefCell<HashMap<i64, Item>>,
        pub stats: RefCell<Stats>,
    }

    impl Default for TorrentModel {
        fn default() -> Self {
            Self {
                store: gio::ListStore::new::<Item>(),
                ht: Default::default(),
                stats: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TorrentModel {
        const NAME: &'static str = "TrgTorrentModel";
        type Type = super::TorrentModel;
    }

    impl ObjectImpl for TorrentModel {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("torrent-completed")
                        .param_types([Item::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("update").build(),
                    glib::subclass::Signal::builder("torrent-added")
                        .param_types([Item::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("torrents-state-change")
                        .param_types([u32::static_type()])
                        .build(),
                ]
            })
        }
    }
}

glib::wrapper! {
    pub struct TorrentModel(ObjectSubclass<imp::TorrentModel>);
}

impl Default for TorrentModel {
    fn default() -> Self {
        Self::new()
    }
}

/// The short version of a download directory: its label among the
/// destinations, "Default" for the session's download directory, or the
/// part below that.
pub fn shorten_download_dir(client: &Client, download_dir: &str) -> String {
    let prefs = client.prefs();
    if let Some(labels) = prefs.get_array(prefs::KEY_DESTINATIONS, Scope::Connection) {
        for label in objects(&labels) {
            if get_str(label, prefs::KEY_DESTINATIONS_SUBKEY_DIR) == Some(download_dir)
                && let Some(l) = get_str(label, prefs::SUBKEY_LABEL)
            {
                return l.to_string();
            }
        }
    }

    let default_dir = client.session().map(|s| s.download_dir.clone()).unwrap_or_default();

    if download_dir == default_dir {
        return tr("Default");
    }

    if !default_dir.is_empty()
        && let Some(rest) = download_dir.strip_prefix(&default_dir)
    {
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        if !rest.is_empty() {
            return rest.to_string();
        }
    }

    download_dir.to_string()
}

impl TorrentModel {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn store(&self) -> gio::ListStore {
        self.imp().store.clone()
    }

    pub fn stats(&self) -> Stats {
        *self.imp().stats.borrow()
    }

    pub fn find(&self, id: i64) -> Option<Item> {
        self.imp().ht.borrow().get(&id).cloned()
    }

    /// The download directories of all the torrents
    pub fn download_dirs(&self) -> Vec<String> {
        self.imp().ht.borrow().values().filter_map(|i| i.string(COL_DOWNLOADDIR)).collect()
    }

    pub fn connect_torrent_completed<F: Fn(&Item) + 'static>(&self, f: F) {
        self.connect_local("torrent-completed", false, move |args| {
            f(&args[1].get::<Item>().unwrap());
            None
        });
    }

    pub fn connect_torrent_added<F: Fn(&Item) + 'static>(&self, f: F) {
        self.connect_local("torrent-added", false, move |args| {
            f(&args[1].get::<Item>().unwrap());
            None
        });
    }

    pub fn connect_update<F: Fn() + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("update", true, move |_| {
            f();
            None
        })
    }

    pub fn connect_state_change<F: Fn(u32) + 'static>(&self, f: F) {
        self.connect_local("torrents-state-change", false, move |args| {
            f(args[1].get::<u32>().unwrap());
            None
        });
    }

    pub fn reload_dir_aliases(&self, client: &Client) {
        for item in item::items(&self.imp().store) {
            let dir = item.string(COL_DOWNLOADDIR).unwrap_or_default();
            item.set(COL_DOWNLOADDIR_SHORT, shorten_download_dir(client, &dir));
        }
        self.emit_by_name::<()>("torrents-state-change", &[&UPDATE_PATH_CHANGE]);
    }

    fn stats_scan(&self) {
        let mut stats = self.imp().stats.borrow_mut();
        let (down_rate, up_rate) = (stats.down_rate_total, stats.up_rate_total);
        *stats = Stats { down_rate_total: down_rate, up_rate_total: up_rate, ..Default::default() };

        for item in item::items(&self.imp().store) {
            let flags = item.uint(COL_FLAGS);
            if flags & p::TORRENT_FLAG_SEEDING != 0 {
                stats.seeding += 1;
            } else if flags & p::TORRENT_FLAG_DOWNLOADING != 0 {
                stats.down += 1;
            } else if flags & p::TORRENT_FLAG_PAUSED != 0 {
                stats.paused += 1;
            }
            if flags & p::TORRENT_FLAG_ERROR != 0 {
                stats.error += 1;
            }
            if flags & p::TORRENT_FLAG_COMPLETE != 0 {
                stats.complete += 1;
            } else {
                stats.incomplete += 1;
            }
            if flags & p::TORRENT_FLAG_CHECKING_ANY != 0 {
                stats.checking += 1;
            }
            if flags & p::TORRENT_FLAG_ACTIVE != 0 {
                stats.active += 1;
            }
            if flags & p::TORRENT_FLAG_SEEDING_WAIT != 0 {
                stats.seed_wait += 1;
            }
            if flags & p::TORRENT_FLAG_DOWNLOADING_WAIT != 0 {
                stats.down_wait += 1;
            }
            stats.count += 1;
        }
    }

    pub fn remove_all(&self) {
        self.imp().ht.borrow_mut().clear();
        self.imp().store.remove_all();
    }

    fn update_item(
        &self,
        client: &Client,
        serial: i64,
        item: &Item,
        t: Rc<Torrent>,
        changed: &mut u32,
    ) {
        {
            let mut stats = self.imp().stats.borrow_mut();
            stats.down_rate_total += t.rate_download;
            stats.up_rate_total += t.rate_upload;
        }

        let download_dir = util::rm_trailing_slashes(&t.download_dir).to_string();
        let new_flags = t.flags();

        let last_flags = item.uint(COL_FLAGS);
        let last_download_dir = item.string(COL_DOWNLOADDIR);

        // like the sidebar's tracker filters (the "host" field is host:port,
        // without a scheme)
        let first_tracker_host =
            t.tracker_stats.first().and_then(|tracker| util::tracker_host(&tracker.announce));

        let pf = &t.peers_from;
        let peer_sources = (new_flags & p::TORRENT_FLAG_ACTIVE != 0).then(|| {
            let lpd = pf.from_lpd.map_or_else(|| "N/A".to_string(), |lpd| lpd.to_string());
            format!(
                "{} / {} / {} / {} / {} / {lpd} / {}",
                pf.from_tracker,
                pf.from_incoming,
                pf.from_ltep,
                pf.from_dht,
                pf.from_pex,
                pf.from_cache
            )
        });

        let percent_done = if new_flags & p::TORRENT_FLAG_CHECKING != 0 {
            t.recheck_percent()
        } else {
            t.done_percent()
        };

        // seeders/leechers: the maximum of the trackers, downloads: the sum
        let trackers = &t.tracker_stats;
        let seeders = trackers.iter().map(|t| t.seeder_count).max().unwrap_or(0).max(0);
        let leechers = trackers.iter().map(|t| t.leecher_count).max().unwrap_or(0).max(0);
        let downloads: i64 = trackers.iter().map(|t| t.download_count).sum();

        let (uploaded, have_valid) = (t.uploaded_ever, t.have_valid);
        item.set(COL_ICON, p::status_icon(new_flags));
        item.set(COL_ADDED, t.added_date);
        item.set(COL_FILECOUNT, t.files.len() as u32);
        item.set(COL_DONE_DATE, t.done_date);
        item.set(COL_NAME, t.name.as_str());
        item.set(COL_ERROR, t.error);
        item.set(COL_SIZEWHENDONE, t.size_when_done);
        item.set(COL_PERCENTDONE, percent_done);
        item.set(COL_METADATAPERCENTCOMPLETE, t.metadata_percent());
        item.set(COL_STATUS, t.status_text(new_flags));
        item.set(COL_DOWNSPEED, t.rate_download);
        item.set(COL_FLAGS, new_flags);
        item.set(COL_UPSPEED, t.rate_upload);
        item.set(COL_ETA, t.eta);
        item.set(COL_UPLOADED, uploaded);
        item.set(COL_DOWNLOADED, t.downloaded_ever);
        item.set(COL_TOTALSIZE, t.total_size);
        item.set(COL_HAVE_UNCHECKED, t.have_unchecked);
        item.set(COL_HAVE_VALID, have_valid);
        item.set(COL_FROMPEX, pf.from_pex);
        item.set(COL_FROMDHT, pf.from_dht);
        item.set(COL_FROMTRACKERS, pf.from_tracker);
        item.set(COL_FROMLTEP, pf.from_ltep);
        item.set(COL_FROMRESUME, pf.from_cache);
        item.set(COL_FROMINCOMING, pf.from_incoming);
        item.set(COL_PEER_SOURCES, peer_sources);
        item.set(COL_PEERS_CONNECTED, t.peers_connected);
        item.set(COL_PEERS_TO_US, t.peers_sending_to_us);
        item.set(COL_PEERS_FROM_US, t.peers_getting_from_us);
        item.set(COL_WEB_SEEDS_TO_US, t.webseeds_sending_to_us);
        item.set(COL_QUEUE_POSITION, t.queue_position.unwrap_or(-1));
        item.set(COL_SEED_RATIO_LIMIT, t.seed_ratio_limit);
        item.set(COL_SEED_RATIO_MODE, t.seed_ratio_mode);
        item.set(COL_LASTACTIVE, t.activity_date);
        item.set(
            COL_RATIO,
            if uploaded > 0 && have_valid > 0 { uploaded as f64 / have_valid as f64 } else { 0.0 },
        );
        item.set(COL_DOWNLOADDIR, download_dir.as_str());
        item.set(COL_BANDWIDTH_PRIORITY, t.bandwidth_priority);
        item.set(COL_ID, t.id);
        item.set(COL_TORRENT, t.clone());
        item.set(COL_TRACKERHOST, first_tracker_host.unwrap_or_default());

        if last_download_dir.as_deref() != Some(download_dir.as_str()) {
            item.set(COL_DOWNLOADDIR_SHORT, shorten_download_dir(client, &download_dir));
            *changed |= UPDATE_PATH_CHANGE;
        }

        item.set(COL_SEEDS, seeders);
        item.set(COL_LEECHERS, leechers);
        item.set(COL_DOWNLOADS, downloads);

        // set last, so a row watching it sees all the changes
        item.set(COL_UPDATESERIAL, serial);

        if last_flags & p::TORRENT_FLAG_DOWNLOADING != 0
            && new_flags & p::TORRENT_FLAG_DOWNLOADING == 0
            && new_flags & p::TORRENT_FLAG_COMPLETE != 0
        {
            self.emit_by_name::<()>("torrent-completed", &[item]);
        }

        if last_flags != new_flags {
            *changed |= UPDATE_STATE_CHANGE;
        }
    }

    /// Removes the stale torrents: those marked with the update serial -1,
    /// and if serial isn't -1, those which weren't updated with it.
    fn remove_stale(&self, serial: i64) -> bool {
        let ht = &self.imp().ht;
        util::remove_where(&self.imp().store, |item: &Item| {
            let s = item.int64(COL_UPDATESERIAL);
            let stale = !(s != -1 && (serial == -1 || s == serial));
            if stale {
                ht.borrow_mut().remove(&item.int64(COL_ID));
            }
            stale
        }) > 0
    }

    /// Updates the torrents from a torrent_get result
    pub fn update(&self, client: &Client, result: Obj, mode: UpdateMode) -> Stats {
        let serial = client.serial();
        let mut changed = 0u32;
        let mut added = Vec::new();

        {
            let mut stats = self.imp().stats.borrow_mut();
            stats.down_rate_total = 0;
            stats.up_rate_total = 0;
        }

        let (torrents, removed) = p::torrents_of(result);
        for t in torrents {
            let t = Rc::new(t);
            let id = t.id;
            let existing = self.find(id);
            match existing {
                None => {
                    let item = new_torrent_item();
                    self.update_item(client, serial, &item, t, &mut changed);
                    self.imp().ht.borrow_mut().insert(id, item.clone());
                    added.push(item);
                    changed |= UPDATE_ADDREMOVE;
                }
                Some(item) => self.update_item(client, serial, &item, t, &mut changed),
            }
        }

        // all the new torrents in one go
        if !added.is_empty() {
            let store = &self.imp().store;
            store.splice(store.n_items(), 0, &added);
        }

        if mode != UpdateMode::First {
            for item in &added {
                self.emit_by_name::<()>("torrent-added", &[item]);
            }
        }

        if mode == UpdateMode::Update {
            if self.remove_stale(serial) {
                changed |= UPDATE_ADDREMOVE;
            }
        } else if mode != UpdateMode::First && !removed.is_empty() {
            for id in removed {
                // mark it, the stale ones are removed below
                if let Some(item) = self.find(id) {
                    item.set(COL_UPDATESERIAL, -1i64);
                }
            }
            self.remove_stale(-1);
            changed |= UPDATE_ADDREMOVE;
        }

        if changed != 0 {
            if changed & (UPDATE_ADDREMOVE | UPDATE_STATE_CHANGE) != 0 {
                self.stats_scan();
            }
            self.emit_by_name::<()>("torrents-state-change", &[&changed]);
        }

        self.emit_by_name::<()>("update", &[]);

        self.stats()
    }
}
