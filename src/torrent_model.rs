//! The torrents: a list of TorrentObjects which updates from a torrent-get
//! response.
//! It handles a number of different update modes:
//!   1) the first update,
//!   2) a full update,
//!   3) an active-only update,
//!   4) individual torrent updates.
//!
//! The objects are updated in place (only the values that changed notify), and
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
use crate::json::{Obj, get_str, objects};
use crate::prefs::{self, Scope};
use crate::protocol::{self as p, Torrent};
use crate::torrent_object::{TorrentObject, TorrentValues};
use crate::util;

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
        pub ht: RefCell<HashMap<i64, TorrentObject>>,
        pub stats: RefCell<Stats>,
    }

    impl Default for TorrentModel {
        fn default() -> Self {
            Self {
                store: gio::ListStore::new::<TorrentObject>(),
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
                        .param_types([TorrentObject::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("update").build(),
                    glib::subclass::Signal::builder("torrent-added")
                        .param_types([TorrentObject::static_type()])
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

    pub fn find(&self, id: i64) -> Option<TorrentObject> {
        self.imp().ht.borrow().get(&id).cloned()
    }

    /// The download directories of all the torrents
    pub fn download_dirs(&self) -> Vec<String> {
        self.imp().ht.borrow().values().map(|t| t.values().download_dir.clone()).collect()
    }

    pub fn connect_torrent_completed<F: Fn(&TorrentObject) + 'static>(&self, f: F) {
        self.connect_local("torrent-completed", false, move |args| {
            f(&args[1].get::<TorrentObject>().unwrap());
            None
        });
    }

    pub fn connect_torrent_added<F: Fn(&TorrentObject) + 'static>(&self, f: F) {
        self.connect_local("torrent-added", false, move |args| {
            f(&args[1].get::<TorrentObject>().unwrap());
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
        for obj in util::objects::<TorrentObject>(&self.imp().store) {
            let mut values = obj.values().clone();
            values.download_dir_short = shorten_download_dir(client, &values.download_dir);
            obj.set_values(values);
        }
        self.emit_by_name::<()>("torrents-state-change", &[&UPDATE_PATH_CHANGE]);
    }

    fn stats_scan(&self) {
        let mut stats = self.imp().stats.borrow_mut();
        let (down_rate, up_rate) = (stats.down_rate_total, stats.up_rate_total);
        *stats = Stats { down_rate_total: down_rate, up_rate_total: up_rate, ..Default::default() };

        for obj in util::objects::<TorrentObject>(&self.imp().store) {
            let flags = obj.values().flags;
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

    fn update_object(
        &self,
        client: &Client,
        serial: i64,
        obj: &TorrentObject,
        t: Rc<Torrent>,
        added: bool,
        changed: &mut u32,
    ) {
        {
            let mut stats = self.imp().stats.borrow_mut();
            stats.down_rate_total += t.rate_download;
            stats.up_rate_total += t.rate_upload;
        }

        let download_dir = util::rm_trailing_slashes(&t.download_dir).to_string();
        let new_flags = t.flags();

        let (last_flags, last_download_dir, last_download_dir_short) = {
            let v = obj.values();
            (v.flags, v.download_dir.clone(), v.download_dir_short.clone())
        };
        let dir_changed = added || last_download_dir != download_dir;

        // like the sidebar's tracker filters (the "host" field is host:port,
        // without a scheme)
        let first_tracker_host =
            t.tracker_stats.first().and_then(|tracker| util::tracker_host(&tracker.announce));

        let pf = &t.peers_from;
        // of the connected peers, while it's running
        let peer_sources = if matches!(t.status, p::TR_STATUS_DOWNLOAD | p::TR_STATUS_SEED) {
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
        } else {
            String::new()
        };

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
        let download_dir_short = if dir_changed {
            *changed |= UPDATE_PATH_CHANGE;
            shorten_download_dir(client, &download_dir)
        } else {
            last_download_dir_short
        };

        obj.set_torrent(t.clone());
        obj.set_values(TorrentValues {
            id: t.id,
            name: t.name.clone(),
            icon: p::status_icon(new_flags).to_string(),
            flags: new_flags,
            status: t.status_text(new_flags),
            percent_done,
            size_when_done: t.size_when_done,
            ratio: if uploaded > 0 && have_valid > 0 {
                uploaded as f64 / have_valid as f64
            } else {
                0.0
            },
            seeders,
            leechers,
            downloads,
            peers_connected: t.peers_connected,
            peers_sending_to_us: t.peers_sending_to_us,
            peers_getting_from_us: t.peers_getting_from_us,
            from_pex: pf.from_pex,
            from_dht: pf.from_dht,
            from_trackers: pf.from_tracker,
            from_ltep: pf.from_ltep,
            from_resume: pf.from_cache,
            from_incoming: pf.from_incoming,
            peer_sources,
            down_speed: t.rate_download,
            up_speed: t.rate_upload,
            eta: t.eta,
            uploaded,
            downloaded: t.downloaded_ever,
            added: t.added_date,
            done_date: t.done_date,
            last_active: t.activity_date,
            tracker_host: first_tracker_host.unwrap_or_default().to_string(),
            download_dir,
            download_dir_short,
            bandwidth_priority: t.bandwidth_priority,
            queue_position: t.queue_position.unwrap_or(-1),
        });

        // set last, so a row watching it sees all the changes
        obj.set_serial(serial);

        if last_flags & p::TORRENT_FLAG_DOWNLOADING != 0
            && new_flags & p::TORRENT_FLAG_DOWNLOADING == 0
            && new_flags & p::TORRENT_FLAG_COMPLETE != 0
        {
            self.emit_by_name::<()>("torrent-completed", &[obj]);
        }

        if last_flags != new_flags {
            *changed |= UPDATE_STATE_CHANGE;
        }
    }

    /// Removes the stale torrents: those marked with the update serial -1,
    /// and if serial isn't -1, those which weren't updated with it.
    fn remove_stale(&self, serial: i64) -> bool {
        let ht = &self.imp().ht;
        util::remove_where(&self.imp().store, |obj: &TorrentObject| {
            let s = obj.serial();
            let stale = !(s != -1 && (serial == -1 || s == serial));
            if stale {
                ht.borrow_mut().remove(&obj.id());
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
                    let obj = TorrentObject::new(t.clone());
                    self.update_object(client, serial, &obj, t, true, &mut changed);
                    self.imp().ht.borrow_mut().insert(id, obj.clone());
                    added.push(obj);
                    changed |= UPDATE_ADDREMOVE;
                }
                Some(obj) => self.update_object(client, serial, &obj, t, false, &mut changed),
            }
        }

        // all the new torrents in one go
        if !added.is_empty() {
            let store = &self.imp().store;
            store.splice(store.n_items(), 0, &added);
        }

        if mode != UpdateMode::First {
            for obj in &added {
                self.emit_by_name::<()>("torrent-added", &[obj]);
            }
        }

        if mode == UpdateMode::Update {
            if self.remove_stale(serial) {
                changed |= UPDATE_ADDREMOVE;
            }
        } else if mode != UpdateMode::First && !removed.is_empty() {
            for id in removed {
                // mark it, the stale ones are removed below
                if let Some(obj) = self.find(id) {
                    obj.set_serial(-1);
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
