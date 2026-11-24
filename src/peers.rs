//! The peers of the selected torrent, updated by IP. The host names are looked
//! up asynchronously if the column is showing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::gio;
use gtk::prelude::*;

use crate::i18n::tr;
use crate::item::{self, Item, Val};
use crate::prefs::Prefs;
use crate::protocol::Torrent;
use crate::torrent_model::UpdateMode;
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

    pub fn update(&self, tv: &TreeView, serial: i64, t: &Torrent, mode: UpdateMode) {
        let do_host_lookup = tv.is_column_showing("host");
        let mut added = Vec::new();

        if mode == UpdateMode::First {
            self.clear();
        }

        for peer in &t.peers {
            let address = peer.address.clone();
            let existing = self.peers.borrow().get(&address).cloned();
            let item = match existing {
                Some(item) => item,
                None => {
                    let item = new_peer_item();
                    item.set(COL_ICON, "network-workgroup");
                    item.set(COL_IP, address.as_str());
                    item.set(COL_CLIENT, peer.client_name.as_str());
                    self.peers.borrow_mut().insert(address.clone(), item.clone());
                    added.push(item.clone());

                    if do_host_lookup && let Some(addr) = gio::InetAddress::from_string(&address) {
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
                    item
                }
            };

            item.set(COL_FLAGS, peer.flag_str.as_str());
            item.set(COL_PROGRESS, peer.progress * 100.0);
            item.set(COL_DOWNSPEED, peer.rate_to_client);
            item.set(COL_UPSPEED, peer.rate_to_peer);
            item.set(COL_UPDATESERIAL, serial);
        }

        if !added.is_empty() {
            self.store.splice(self.store.n_items(), 0, &added);
        }

        if mode != UpdateMode::First {
            let mut peers = self.peers.borrow_mut();
            for item in item::items(&self.store) {
                if item.int64(COL_UPDATESERIAL) != serial
                    && let Some(ip) = item.string(COL_IP)
                {
                    peers.remove(&ip);
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

    reg(ColType::IconText, COL_IP, &tr("IP"), "ip").set_extra(COL_ICON);
    reg(ColType::Text, COL_HOST, &tr("Host"), "host");
    reg(ColType::Speed, COL_DOWNSPEED, &tr("Down Speed"), "down-speed");
    reg(ColType::Speed, COL_UPSPEED, &tr("Up Speed"), "up-speed");
    reg(ColType::Prog, COL_PROGRESS, &tr("Progress"), "progress");
    reg(ColType::Text, COL_FLAGS, &tr("Flags"), "flags");
    reg(ColType::Text, COL_CLIENT, &tr("Client"), "client");

    tv.set_search_column(COL_HOST);
    tv.set_model(&model.store);
    tv.setup_columns();
    tv.restore_sort();
    tv
}
