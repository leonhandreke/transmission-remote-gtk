//! The peers of the selected torrent, updated by IP (trg-peers-model.c and
//! trg-peers-tree-view.c). The host names are looked up asynchronously if
//! the column is showing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::gio;
use gtk::prelude::*;

use crate::i18n::tr;
use crate::item::{self, Item, Val};
use crate::prefs::Prefs;
use crate::protocol::{self as p, Obj, get_i64, get_str};
use crate::tree_view::{ColType, TreeView};

pub const COL_ICON: usize = 0;
pub const COL_IP: usize = 1;
pub const COL_HOST: usize = 2;
pub const COL_FLAGS: usize = 3;
pub const COL_PROGRESS: usize = 4;
pub const COL_DOWNSPEED: usize = 5;
pub const COL_UPSPEED: usize = 6;
pub const COL_CLIENT: usize = 7;
pub const COL_UPDATESERIAL: usize = 8;

fn new_peer_item() -> Item {
    Item::new(vec![
        Val::Str(None),
        Val::Str(None),
        Val::Str(None),
        Val::Str(None),
        Val::F64(0.0),
        Val::I64(0),
        Val::I64(0),
        Val::Str(None),
        Val::I64(0),
    ])
}

pub struct PeersModel {
    pub store: gio::ListStore,
    /// IP -> item
    peers: RefCell<HashMap<String, Item>>,
}

impl PeersModel {
    pub fn new() -> Rc<Self> {
        Rc::new(Self { store: gio::ListStore::new::<Item>(), peers: Default::default() })
    }

    pub fn clear(&self) {
        self.peers.borrow_mut().clear();
        self.store.remove_all();
    }

    pub fn update(&self, tv: &TreeView, serial: i64, t: &Obj, mode: i32) {
        let do_host_lookup = tv.is_column_showing(COL_HOST as i32);
        let mut added = Vec::new();

        if mode == p::TORRENT_GET_MODE_FIRST {
            self.clear();
        }

        for peer in p::objects(p::torrent_get_peers(t)) {
            let address = get_str(peer, p::TPEER_ADDRESS).unwrap_or("").to_string();
            let existing = self.peers.borrow().get(&address).cloned();
            let item = match existing {
                Some(item) => item,
                None => {
                    let item = new_peer_item();
                    item.set(COL_ICON, "network-workgroup");
                    item.set(COL_IP, address.as_str());
                    item.set(COL_CLIENT, get_str(peer, p::TPEER_CLIENT_NAME));
                    self.peers.borrow_mut().insert(address.clone(), item.clone());
                    added.push(item.clone());

                    if do_host_lookup {
                        if let Some(addr) = gio::InetAddress::from_string(&address) {
                            let weak = item.downgrade();
                            gio::Resolver::default().lookup_by_address_async(
                                &addr,
                                gio::Cancellable::NONE,
                                move |res| {
                                    if let (Ok(name), Some(item)) = (res, weak.upgrade()) {
                                        item.set(COL_HOST, name.as_str());
                                    }
                                },
                            );
                        }
                    }
                    item
                }
            };

            item.set(COL_FLAGS, get_str(peer, p::TPEER_FLAGSTR));
            item.set(COL_PROGRESS, p::json_double_to_progress(peer.get(p::TPEER_PROGRESS)));
            item.set(COL_DOWNSPEED, get_i64(peer, p::TPEER_RATE_TO_CLIENT));
            item.set(COL_UPSPEED, get_i64(peer, p::TPEER_RATE_TO_PEER));
            item.set(COL_UPDATESERIAL, serial);
        }

        if !added.is_empty() {
            self.store.splice(self.store.n_items(), 0, &added);
        }

        if mode != p::TORRENT_GET_MODE_FIRST {
            let mut peers = self.peers.borrow_mut();
            for item in item::items(&self.store) {
                if item.int64(COL_UPDATESERIAL) != serial {
                    if let Some(ip) = item.string(COL_IP) {
                        peers.remove(&ip);
                    }
                }
            }
            drop(peers);
            item::remove_removed(&self.store, COL_UPDATESERIAL, serial);
        }
    }
}

pub fn peers_tree_view_new(prefs: &Prefs, model: &PeersModel, config_id: Option<&str>) -> TreeView {
    let tv = TreeView::new(Some(prefs), config_id.unwrap_or("TrgPeersTreeView"));
    let reg = |t, c: usize, h: &str, id: &str| tv.reg_column(t, c as i32, h, id, 0);

    reg(ColType::IconText, COL_IP, &tr("IP"), "ip").set_extra(COL_ICON as i32);
    reg(ColType::Text, COL_HOST, &tr("Host"), "host");
    reg(ColType::Speed, COL_DOWNSPEED, &tr("Down Speed"), "down-speed");
    reg(ColType::Speed, COL_UPSPEED, &tr("Up Speed"), "up-speed");
    reg(ColType::Prog, COL_PROGRESS, &tr("Progress"), "progress");
    reg(ColType::Text, COL_FLAGS, &tr("Flags"), "flags");
    reg(ColType::Text, COL_CLIENT, &tr("Client"), "client");

    tv.set_search_column(COL_HOST as i32);
    tv.set_model(&model.store);
    tv.setup_columns();
    tv.restore_sort();
    tv
}
