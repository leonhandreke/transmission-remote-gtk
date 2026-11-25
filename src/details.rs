//! The details of a torrent. `Details` is the notebook below the torrent
//! list, which shows the selected torrent: the General page and the lists.
//! `DetailLists` are the lists of its trackers, files and peers, which update
//! from its JSON; the properties dialog shows them too.

use std::cell::OnceCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::client::{Client, ConnectionState};
use crate::files::{FilesModel, FilesView};
use crate::general_panel::GeneralPanel;
use crate::i18n::tr;
use crate::peers::{self, PeersModel};
use crate::protocol::Torrent;
use crate::torrent_model::UpdateMode;
use crate::torrent_object::TorrentObject;
use crate::trackers::{TrackersModel, TrackersView};
use crate::tree_view::{PERSIST_LAYOUT, PERSIST_SORT, TreeView};

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/details.ui")]
    pub struct Details {
        #[template_child]
        pub notebook: TemplateChild<gtk::Notebook>,
        #[template_child]
        pub general: TemplateChild<GeneralPanel>,

        pub client: OnceCell<Client>,
        pub lists: OnceCell<DetailLists>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Details {
        const NAME: &'static str = "TrgDetails";
        type Type = super::Details;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            GeneralPanel::ensure_type();
            klass.bind_template();
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Details {
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for Details {}
}

glib::wrapper! {
    pub struct Details(ObjectSubclass<imp::Details>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Details {
    /// Shows the details of the client's torrents. The lists are
    /// insensitive until it's connected, see set_connection().
    pub fn set_client(&self, client: &Client) {
        let imp = self.imp();
        imp.client.set(client.clone()).expect("the client is set once");
        let lists = DetailLists::new(client, "");
        for (tv, title) in lists.pages() {
            imp.notebook.append_page(tv, Some(&gtk::Label::new(Some(&title))));
        }
        lists.set_sensitive(false);
        if imp.lists.set(lists).is_err() {
            unreachable!("the lists are made once");
        }
    }

    fn client(&self) -> &Client {
        self.imp().client.get().expect("the client is set by set_client()")
    }

    fn lists(&self) -> &DetailLists {
        self.imp().lists.get().expect("the lists are made by set_client()")
    }

    /// The General page, which is gone once the details are disposed (the
    /// torrents can change after that: the model is the client's)
    fn general(&self) -> Option<GeneralPanel> {
        self.imp().general.try_get()
    }

    /// Shows the torrent (selected, or updated as mode says), or nothing
    pub fn set_torrent(&self, torrent: Option<&TorrentObject>, mode: UpdateMode) {
        let Some(obj) = torrent else {
            self.clear();
            return;
        };
        let t = obj.torrent();
        if let Some(general) = self.general() {
            general.update(&t, &obj.values());
        }
        self.lists().update(&t, self.client().serial(), mode);
    }

    fn clear(&self) {
        self.lists().clear();
        if let Some(general) = self.general() {
            general.clear();
        }
    }

    /// The details are sensitive while connected, and show nothing once
    /// disconnected
    pub fn set_connection(&self, state: ConnectionState) {
        let connected = state == ConnectionState::Connected;
        self.lists().set_sensitive(connected);
        if let Some(general) = self.general() {
            general.set_sensitive(connected);
        }
        if state == ConnectionState::Disconnected {
            self.clear();
        }
    }

    /// Saves the lists' column layout and sorting
    pub fn persist(&self) {
        self.lists().persist();
    }
}

/// The lists of a torrent's trackers, files and peers
pub struct DetailLists {
    trackers_model: Rc<TrackersModel>,
    trackers_view: Rc<TrackersView>,
    files_model: Rc<FilesModel>,
    files_view: Rc<FilesView>,
    peers_model: Rc<PeersModel>,
    peers_tv: TreeView,
}

impl DetailLists {
    /// The lists, whose settings are saved under their names followed by
    /// config_suffix
    pub fn new(client: &Client, config_suffix: &str) -> Self {
        let trackers_model = TrackersModel::new();
        let trackers_view = TrackersView::new(
            &trackers_model,
            client,
            Some(&format!("TrgTrackersTreeView{config_suffix}")),
        );
        let files_model = FilesModel::new();
        let files_view =
            FilesView::new(&files_model, client, Some(&format!("TrgFilesTreeView{config_suffix}")));
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
    pub fn update(&self, t: &Torrent, serial: i64, mode: UpdateMode) {
        self.trackers_model.update(serial, t, mode);
        self.files_model.update(t, mode);
        self.peers_model.update(&self.peers_tv, serial, t, mode);
    }

    /// Shows no torrent
    fn clear(&self) {
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
