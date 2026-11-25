//! The torrent list: the client's torrents that the sidebar's filter and the
//! search text show, "Not Connected" over it while not connected, and "No
//! Results" when nothing matches the search. The search entry is the list's,
//! the main window shows it in its toolbar.

use std::cell::{Cell, OnceCell, RefCell};
use std::sync::OnceLock;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::client::{Client, ConnectionState};
use crate::i18n::{sprintf, tr};
use crate::prefs::{self, Scope};
use crate::state_selector::Filter;
use crate::torrent_object::TorrentObject;
use crate::torrent_view;
use crate::tree_view::{PERSIST_LAYOUT, PERSIST_SORT, TreeView};
use crate::util;

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/torrent-list.ui")]
    pub struct TorrentList {
        #[template_child]
        pub overlay: TemplateChild<gtk::Overlay>,
        #[template_child]
        pub no_results: TemplateChild<gtk::Box>,
        #[template_child]
        pub not_connected: TemplateChild<gtk::Box>,
        #[template_child]
        pub not_connected_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub connecting_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub connecting_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,

        pub client: OnceCell<Client>,
        pub tv: OnceCell<TreeView>,
        pub torrent_filter: OnceCell<gtk::CustomFilter>,

        // what the torrent filter shows: the state selector's filter, and
        // the entry's text (casefolded)
        pub filter: RefCell<Filter>,
        pub filter_text: RefCell<Option<String>>,
        // the first selected torrent (-1 for none), as last signalled
        pub selected_id: Cell<i64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TorrentList {
        const NAME: &'static str = "TrgTorrentList";
        type Type = super::TorrentList;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TorrentList {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("selection-changed").build(),
                    glib::subclass::Signal::builder("row-activated").build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.selected_id.set(-1);
            // Not wider than the list, when it's narrow
            self.overlay.set_clip_overlay(&*self.no_results, true);
            self.overlay.set_clip_overlay(&*self.not_connected, true);
            self.obj().setup_search();
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for TorrentList {
        fn grab_focus(&self) -> bool {
            self.tv.get().is_some_and(|tv| tv.grab_focus())
        }
    }
}

glib::wrapper! {
    pub struct TorrentList(ObjectSubclass<imp::TorrentList>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TorrentList {
    /// Shows the client's torrents. The list is insensitive until it's
    /// connected, see set_connection().
    pub fn set_client(&self, client: &Client) {
        let imp = self.imp();
        imp.client.set(client.clone()).expect("the client is set once");

        // the torrents shown, filtered by the state selector and the entry
        let weak = self.downgrade();
        let filter = gtk::CustomFilter::new(move |obj| {
            let (Some(list), Some(torrent)) = (weak.upgrade(), obj.downcast_ref::<TorrentObject>())
            else {
                return true;
            };
            list.visible_func(torrent)
        });
        let model = client.torrent_model();
        let filtered = gtk::FilterListModel::new(Some(model.store()), Some(filter.clone()));
        let tv = torrent_view::torrent_tree_view_new(client, &filtered);
        imp.torrent_filter.set(filter).expect("the filter is made once");
        imp.tv.set(tv.clone()).expect("the tree view is made once");
        imp.overlay.set_child(Some(&tv));

        let selection = tv.selection();
        let weak = self.downgrade();
        selection.connect_selection_changed(move |_, _, _| {
            if let Some(list) = weak.upgrade() {
                list.selection_changed();
            }
        });
        // Torrents were added, removed, filtered or sorted. When the selected
        // torrent is gone, so is the selection, without a selection-changed.
        let weak = self.downgrade();
        selection.connect_items_changed(move |_, _, _, _| {
            if let Some(list) = weak.upgrade() {
                if list.first_selected_id() != list.selected_id() {
                    list.selection_changed();
                }
                list.update_no_results();
            }
        });
        let weak = self.downgrade();
        tv.connect_row_activated(move |_, _| {
            if let Some(list) = weak.upgrade() {
                list.emit_by_name::<()>("row-activated", &[]);
            }
        });

        // enabled once connected
        tv.set_sensitive(false);
    }

    fn client(&self) -> Client {
        self.imp().client.get().expect("the client is set by set_client()").clone()
    }

    /// The tree view, for its sort menu
    pub fn tree_view(&self) -> TreeView {
        self.imp().tv.get().expect("the tree view is made by set_client()").clone()
    }

    /// The search entry, which the window shows in its toolbar
    pub fn search_entry(&self) -> gtk::SearchEntry {
        self.imp().search_entry.get()
    }

    pub fn focus_search(&self) {
        self.imp().search_entry.grab_focus();
    }

    fn setup_search(&self) {
        let entry = &self.imp().search_entry;
        let weak = self.downgrade();
        entry.connect_search_changed(move |_| {
            if let Some(list) = weak.upgrade() {
                list.refilter();
            }
        });
        // Typing elsewhere in the window started a search: the next keys go
        // to the entry, without replacing what was typed.
        entry.connect_search_started(|entry| {
            if let Some(text) = entry.delegate().and_downcast::<gtk::Text>() {
                text.grab_focus_without_selecting();
            }
        });
        // Escape clears the search, and then goes back to the list.
        let weak = self.downgrade();
        entry.connect_stop_search(move |entry| {
            if !entry.text().is_empty() {
                entry.set_text("");
            } else if let Some(list) = weak.upgrade() {
                list.tree_view().grab_focus();
            }
        });
    }

    /// Persists the columns and the sorting
    pub fn persist(&self) {
        self.tree_view().persist(PERSIST_SORT | PERSIST_LAYOUT);
    }

    /// The selection changed, or the first selected torrent did
    pub fn connect_selection_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_local("selection-changed", false, move |args| {
            f(&args[0].get::<Self>().unwrap());
            None
        });
    }

    /// A torrent was double-clicked, or Enter pressed on it
    pub fn connect_row_activated<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_local("row-activated", false, move |args| {
            f(&args[0].get::<Self>().unwrap());
            None
        });
    }

    fn selection_changed(&self) {
        self.imp().selected_id.set(self.first_selected_id());
        self.emit_by_name::<()>("selection-changed", &[]);
    }

    /// The ID of the first selected torrent (-1 for none), as of the last
    /// selection-changed
    pub fn selected_id(&self) -> i64 {
        self.imp().selected_id.get()
    }

    fn first_selected_id(&self) -> i64 {
        self.first_selected().map(|t| t.id()).unwrap_or(-1)
    }

    pub fn first_selected(&self) -> Option<TorrentObject> {
        self.tree_view().first_selected::<TorrentObject>()
    }

    /// The IDs of the selected torrents
    pub fn selected_ids(&self) -> Vec<i64> {
        torrent_view::selected_ids(&self.tree_view())
    }

    /// Which torrents to show, the state selector's filter
    pub fn set_filter(&self, filter: Filter) {
        *self.imp().filter.borrow_mut() = filter;
        self.refilter();
    }

    /// After an update: the filter and sorter don't watch the torrents, so
    /// the values they look at may have changed.
    pub fn refilter_and_resort(&self) {
        let imp = self.imp();
        let Some(filter) = imp.torrent_filter.get() else { return };
        if *imp.filter.borrow() != Filter::All || imp.filter_text.borrow().is_some() {
            filter.changed(gtk::FilterChange::Different);
        }
        self.tree_view().resort();
    }

    fn visible_func(&self, torrent: &TorrentObject) -> bool {
        let imp = self.imp();
        let v = torrent.values();

        let shown = match &*imp.filter.borrow() {
            Filter::All => true,
            Filter::State(flags) => v.flags & flags != 0,
            Filter::Tracker(host) => torrent.torrent().has_tracker(host),
            Filter::Directory(dir) => v.download_dir_short == *dir,
        };
        if !shown {
            return false;
        }

        if let Some(text) = imp.filter_text.borrow().as_ref() {
            return glib::casefold(&v.name).contains(text.as_str());
        }

        true
    }

    /// The filter or the search text changed
    fn refilter(&self) {
        let imp = self.imp();
        let text = imp.search_entry.text();
        *imp.filter_text.borrow_mut() =
            (!text.is_empty()).then(|| glib::casefold(&text).to_string());

        let Some(filter) = imp.torrent_filter.get() else { return };
        filter.changed(gtk::FilterChange::Different);
        self.update_no_results();
    }

    /// "No Results" over the list, when there's search text and no torrent
    /// matches it
    fn update_no_results(&self) {
        let imp = self.imp();
        // The torrents can change after the list is disposed (and its
        // template children with it): the model is the client's
        let (Some(tv), Some(entry)) = (imp.tv.get(), imp.search_entry.try_get()) else { return };
        let none = !entry.text().is_empty()
            && tv.selection().n_items() == 0
            && self.client().is_connected();
        imp.no_results.set_visible(none);
    }

    /// The list is sensitive while connected, otherwise "Not Connected" is
    /// over it. Its button connects to the current profile (named
    /// profile_name), or opens the profiles if it has no host yet.
    pub fn set_connection(&self, state: ConnectionState, profile_name: &str) {
        let imp = self.imp();
        let prefs = self.client().prefs().clone();
        self.tree_view().set_sensitive(state == ConnectionState::Connected);

        let has_host =
            prefs.get_string(prefs::KEY_HOSTNAME, Scope::Profile).is_some_and(|h| !h.is_empty());
        let stack = &imp.not_connected_stack;
        let page = match state {
            ConnectionState::Connecting => "connecting",
            _ if has_host => "connect",
            _ => "edit",
        };
        if page == "connecting" && stack.visible_child_name().as_deref() != Some(page) {
            util::restart_spinner(&imp.connecting_box);
        }
        stack.set_visible_child_name(page);
        imp.connecting_label.set_label(&sprintf(&tr("Connecting to %s…"), &[&profile_name]));
        imp.not_connected.set_visible(state != ConnectionState::Connected);
        self.update_no_results();
    }
}
