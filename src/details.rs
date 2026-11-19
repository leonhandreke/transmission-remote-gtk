//! The lists of a torrent's details: its trackers, files and peers, which
//! update from its JSON. They're pages of the main window's notebook (after
//! the General page) and of the properties dialog.

use std::rc::Rc;

use gtk::prelude::*;

use crate::client::Client;
use crate::files::{FilesModel, FilesView};
use crate::i18n::tr;
use crate::json::Obj;
use crate::main_window::MainWindow;
use crate::peers::{self, PeersModel};
use crate::torrent_model::UpdateMode;
use crate::trackers::{TrackersModel, TrackersView};
use crate::tree_view::{PERSIST_LAYOUT, PERSIST_SORT, TreeView};

pub struct DetailLists {
    pub trackers_model: Rc<TrackersModel>,
    pub trackers_view: Rc<TrackersView>,
    pub files_model: Rc<FilesModel>,
    pub files_view: Rc<FilesView>,
    pub peers_model: Rc<PeersModel>,
    pub peers_tv: TreeView,
}

impl DetailLists {
    /// The lists, whose settings are saved under their names followed by
    /// config_suffix
    pub fn new(win: &MainWindow, client: &Client, config_suffix: &str) -> Self {
        let trackers_model = TrackersModel::new();
        let trackers_view = TrackersView::new(
            &trackers_model,
            client,
            Some(&format!("TrgTrackersTreeView{config_suffix}")),
        );
        let files_model = FilesModel::new();
        let files_view = FilesView::new(
            &files_model,
            win,
            client,
            Some(&format!("TrgFilesTreeView{config_suffix}")),
        );
        let peers_model = PeersModel::new();
        let peers_tv = peers::peers_tree_view_new(
            client.prefs(),
            &peers_model,
            Some(&format!("TrgPeersTreeView{config_suffix}")),
        );
        Self { trackers_model, trackers_view, files_model, files_view, peers_model, peers_tv }
    }

    /// The lists, with the titles of their pages
    pub fn pages(&self) -> [(&TreeView, String); 3] {
        [
            (&self.trackers_view.tv, tr("Trackers")),
            (&self.files_view.tv, tr("Files")),
            (&self.peers_tv, tr("Peers")),
        ]
    }

    fn views(&self) -> [&TreeView; 3] {
        self.pages().map(|(tv, _)| tv)
    }

    /// Shows the torrent t, from a torrent_get with serial
    pub fn update(&self, t: &Obj, serial: i64, mode: UpdateMode) {
        self.trackers_model.update(serial, t, mode);
        self.files_model.update(t, mode);
        self.peers_model.update(&self.peers_tv, serial, t, mode);
    }

    /// Shows no torrent
    pub fn clear(&self) {
        self.files_model.clear();
        self.trackers_model.clear();
        self.peers_model.clear();
        self.trackers_model.set_no_selection();
    }

    pub fn set_sensitive(&self, sensitive: bool) {
        for tv in self.views() {
            tv.set_sensitive(sensitive);
        }
    }

    /// Saves the lists' column layout and sorting
    pub fn persist(&self) {
        for tv in self.views() {
            tv.persist(PERSIST_SORT | PERSIST_LAYOUT);
        }
    }
}
