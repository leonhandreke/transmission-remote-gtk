//! A torrent of the torrent model: the torrent of its last update, and the
//! values the views show, which are its (read-only) properties.
//!
//! The values are a plain struct, `TorrentValues`, which the code that goes
//! through all the torrents reads directly (sorting, filtering, counting):
//! reading a property is much slower. Setting the values notifies the
//! properties that changed, which is what the cells of the views watch, and
//! then "serial", which changes with every update of the torrent.

use std::cell::{Cell, Ref, RefCell};
use std::cmp::Ordering;
use std::rc::Rc;
use std::sync::OnceLock;

use gtk::glib;
use gtk::glib::subclass::prelude::*;
use gtk::prelude::*;

use crate::protocol::Torrent;
use crate::tree_view::SortKey;

/// Defines TorrentValues with the fields, each a property with the name
macro_rules! torrent_values {
    ($($(#[$doc:meta])* $field:ident: $ty:ty = $name:literal,)*) => {
        /// The values of a torrent the views show (see the top)
        #[derive(Clone, Debug, Default)]
        pub struct TorrentValues {
            $($(#[$doc])* pub $field: $ty,)*
        }

        impl TorrentValues {
            /// A property for each field, in their order
            fn param_specs() -> Vec<glib::ParamSpec> {
                vec![$(<$ty as glib::HasParamSpec>::param_spec_builder()($name)
                    .read_only()
                    .build(),)*]
            }

            /// The value of the property
            fn value(&self, property: &str) -> glib::Value {
                match property {
                    $($name => self.$field.to_value(),)*
                    _ => unreachable!("no property {property}"),
                }
            }

            /// Whether each field differs from that of other, in their order
            fn changes(&self, other: &Self) -> Vec<bool> {
                vec![$(self.$field != other.$field,)*]
            }

            /// Compares the values of the property, for sorting by it
            pub fn compare_fn(property: &str) -> Option<fn(&Self, &Self) -> Ordering> {
                match property {
                    $($name => Some(|a, b| SortKey::compare(&a.$field, &b.$field)),)*
                    _ => None,
                }
            }
        }
    };
}

torrent_values! {
    id: i64 = "id",
    name: String = "name",
    /// The icon name of its status
    icon: String = "icon",
    /// TORRENT_FLAG_*
    flags: u32 = "flags",
    /// Its status, like "Downloading"
    status: String = "status",
    /// In percent, of checking it while it's being checked
    percent_done: f64 = "percent-done",
    size_when_done: i64 = "size-when-done",
    /// Uploaded / valid, 0 if either is 0
    ratio: f64 = "ratio",
    /* the trackers': the most seeders and leechers, the sum of downloads */
    seeders: i64 = "seeders",
    leechers: i64 = "leechers",
    downloads: i64 = "downloads",
    peers_connected: i64 = "peers-connected",
    peers_sending_to_us: i64 = "peers-sending-to-us",
    peers_getting_from_us: i64 = "peers-getting-from-us",
    /* where the peers came from */
    from_pex: i64 = "from-pex",
    from_dht: i64 = "from-dht",
    from_trackers: i64 = "from-trackers",
    from_ltep: i64 = "from-ltep",
    from_resume: i64 = "from-resume",
    from_incoming: i64 = "from-incoming",
    /// "tracker / incoming / LTEP / DHT / PEX / LPD / resume" while it's
    /// active, empty otherwise
    peer_sources: String = "peer-sources",
    /// Bytes per second
    down_speed: i64 = "down-speed",
    up_speed: i64 = "up-speed",
    /// Seconds, or -1 if unknown, -2 if it never ends
    eta: i64 = "eta",
    uploaded: i64 = "uploaded",
    downloaded: i64 = "downloaded",
    /* dates, in seconds since the epoch */
    added: i64 = "added",
    done_date: i64 = "done-date",
    last_active: i64 = "last-active",
    /// The host of its first tracker, like the sidebar's tracker filters
    tracker_host: String = "tracker-host",
    /// Without trailing slashes
    download_dir: String = "download-dir",
    /// Its label among the destinations, or the part below the default one
    /// (see torrent_model::shorten_download_dir())
    download_dir_short: String = "download-dir-short",
    bandwidth_priority: i64 = "bandwidth-priority",
    /// -1 if the server has no queues
    queue_position: i64 = "queue-position",
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TorrentObject {
        pub torrent: RefCell<Rc<Torrent>>,
        pub values: RefCell<TorrentValues>,
        /// The serial of the update it was last in, -1 once it's removed
        pub serial: Cell<i64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TorrentObject {
        const NAME: &'static str = "TrgTorrentObject";
        type Type = super::TorrentObject;
    }

    impl ObjectImpl for TorrentObject {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: OnceLock<Vec<glib::ParamSpec>> = OnceLock::new();
            PROPERTIES.get_or_init(|| {
                let mut specs = TorrentValues::param_specs();
                specs.push(glib::ParamSpecInt64::builder("serial").read_only().build());
                specs
            })
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "serial" => self.serial.get().to_value(),
                name => self.values.borrow().value(name),
            }
        }
    }
}

glib::wrapper! {
    pub struct TorrentObject(ObjectSubclass<imp::TorrentObject>);
}

impl TorrentObject {
    pub fn new(torrent: Rc<Torrent>) -> Self {
        let obj: Self = glib::Object::new();
        *obj.imp().torrent.borrow_mut() = torrent;
        obj
    }

    /// The torrent of its last update
    pub fn torrent(&self) -> Rc<Torrent> {
        self.imp().torrent.borrow().clone()
    }

    pub fn set_torrent(&self, torrent: Rc<Torrent>) {
        *self.imp().torrent.borrow_mut() = torrent;
    }

    pub fn values(&self) -> Ref<'_, TorrentValues> {
        self.imp().values.borrow()
    }

    pub fn id(&self) -> i64 {
        self.values().id
    }

    /// Sets the values, notifying the properties that changed
    pub fn set_values(&self, values: TorrentValues) {
        let imp = self.imp();
        let changes = imp.values.borrow().changes(&values);
        *imp.values.borrow_mut() = values;
        let specs = <imp::TorrentObject as ObjectImpl>::properties();
        for (spec, changed) in specs.iter().zip(changes) {
            if changed {
                self.notify_by_pspec(spec);
            }
        }
    }

    pub fn serial(&self) -> i64 {
        self.imp().serial.get()
    }

    /// Sets the serial of the update, which notifies even if it's the same:
    /// set last in an update, a row watching it sees all the changes.
    pub fn set_serial(&self, serial: i64) {
        self.imp().serial.set(serial);
        self.notify("serial");
    }
}
