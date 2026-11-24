//! The peers of the selected torrent, updated by IP. The host names are looked
//! up asynchronously if the column is showing.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

use crate::i18n::tr;
use crate::prefs::Prefs;
use crate::protocol::Torrent;
use crate::torrent_model::UpdateMode;
use crate::tree_view::{ColType, TreeView};
use crate::util::{self, set_changed};

/* A peer, by its IP */

mod imp {
    use super::*;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::PeerObject)]
    pub struct PeerObject {
        #[property(get, construct_only)]
        pub icon: RefCell<String>,
        #[property(get, construct_only)]
        pub ip: RefCell<String>,
        /// Its host name, once it's looked up
        #[property(get, set)]
        pub host: RefCell<String>,
        #[property(get, set)]
        pub flags: RefCell<String>,
        /// In percent
        #[property(get, set)]
        pub progress: Cell<f64>,
        #[property(get, set)]
        pub down_speed: Cell<i64>,
        #[property(get, set)]
        pub up_speed: Cell<i64>,
        #[property(get, construct_only)]
        pub client: RefCell<String>,
        /// The serial of the update it was last in
        #[property(get, set)]
        pub serial: Cell<i64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PeerObject {
        const NAME: &'static str = "TrgPeerObject";
        type Type = super::PeerObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for PeerObject {}
}

glib::wrapper! {
    pub struct PeerObject(ObjectSubclass<imp::PeerObject>);
}

impl PeerObject {
    fn new(ip: &str, client: &str) -> Self {
        glib::Object::builder()
            .property("icon", "network-workgroup")
            .property("ip", ip)
            .property("client", client)
            .build()
    }
}

pub struct PeersModel {
    pub store: gio::ListStore,
    /// IP -> peer
    peers: RefCell<HashMap<String, PeerObject>>,
}

impl PeersModel {
    pub fn new() -> Rc<Self> {
        Rc::new(Self { store: gio::ListStore::new::<PeerObject>(), peers: Default::default() })
    }

    pub fn clear(&self) {
        self.peers.borrow_mut().clear();
        self.store.remove_all();
    }

    pub fn update(&self, tv: &TreeView, serial: i64, t: &Torrent, mode: UpdateMode) {
        let do_host_lookup = tv.is_column_showing("host");
        let mut added = Vec::new();

        if mode == UpdateMode::First {
            self.clear();
        }

        for peer in &t.peers {
            let address = peer.address.clone();
            let existing = self.peers.borrow().get(&address).cloned();
            let obj = match existing {
                Some(obj) => obj,
                None => {
                    let obj = PeerObject::new(&address, &peer.client_name);
                    self.peers.borrow_mut().insert(address.clone(), obj.clone());
                    added.push(obj.clone());

                    if do_host_lookup && let Some(addr) = gio::InetAddress::from_string(&address) {
                        let weak = obj.downgrade();
                        gio::Resolver::default().lookup_by_address_async(
                            &addr,
                            gio::Cancellable::NONE,
                            move |res| {
                                if let (Ok(name), Some(obj)) = (res, weak.upgrade()) {
                                    set_changed(&obj, "host", name.to_string());
                                }
                            },
                        );
                    }
                    obj
                }
            };

            set_changed(&obj, "flags", peer.flag_str.clone());
            set_changed(&obj, "progress", peer.progress * 100.0);
            set_changed(&obj, "down-speed", peer.rate_to_client);
            set_changed(&obj, "up-speed", peer.rate_to_peer);
            obj.set_serial(serial);
        }

        if !added.is_empty() {
            self.store.splice(self.store.n_items(), 0, &added);
        }

        if mode != UpdateMode::First {
            let mut peers = self.peers.borrow_mut();
            util::remove_where(&self.store, |obj: &PeerObject| {
                let stale = obj.serial() != serial;
                if stale {
                    peers.remove(&obj.ip());
                }
                stale
            });
        }
    }
}

pub fn peers_tree_view_new(prefs: &Prefs, model: &PeersModel, config_id: Option<&str>) -> TreeView {
    let tv = TreeView::new(Some(prefs), config_id.unwrap_or("TrgPeersTreeView"));
    // the keys are the model columns the peers had as Items
    let reg = |t, key, p, h: &str, id: &str| tv.reg_property(t, key, p, h, id, 0);

    reg(ColType::IconText, 1, "ip", &tr("IP"), "ip").set_extra("icon");
    reg(ColType::Text, 2, "host", &tr("Host"), "host");
    reg(ColType::Speed, 5, "down-speed", &tr("Down Speed"), "down-speed");
    reg(ColType::Speed, 6, "up-speed", &tr("Up Speed"), "up-speed");
    reg(ColType::Prog, 4, "progress", &tr("Progress"), "progress");
    reg(ColType::Text, 3, "flags", &tr("Flags"), "flags");
    reg(ColType::Text, 7, "client", &tr("Client"), "client");

    tv.set_search_column("host");
    tv.set_model(&model.store);
    tv.setup_columns();
    tv.restore_sort();
    tv
}
