//! The properties of the selected torrents (trg-torrent-props-dialog.c).
//!
//! The torrent list is passed into here, which this gets the selection from.
//! If there are multiple selections, the first populates the fields. The IDs
//! are taken when the dialog is created, in case the selection changes before
//! clicking OK. Then the values of the widgets are sent with the IDs.
//!
//! In the Transmission styles, a single torrent also has pages of
//! information, files, peers and trackers, which keep themselves up to date.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::client::Client;
use crate::dialog::{self, Dialog, DialogImpl, TrgDialogExt};
use crate::dialogs::json_widgets::JsonWidgets;
use crate::files::{FilesModel, FilesView};
use crate::i18n::{sprintf, tr};
use crate::item::Item;
use crate::main_window::MainWindow;
use crate::peers::{self, PeersModel};
use crate::prefs;
use crate::protocol::{self as p, Obj, get_bool, get_i64, get_str};
use crate::torrent_model as tm;
use crate::torrent_view;
use crate::trackers::{TrackersModel, TrackersView};
use crate::tree_view::{PERSIST_LAYOUT, PERSIST_SORT, TreeView};
use crate::util;

#[derive(Default)]
pub struct Details {
    files_model: Option<Rc<FilesModel>>,
    files_view: Option<Rc<FilesView>>,
    peers_model: Option<Rc<PeersModel>>,
    peers_tv: Option<TreeView>,
    trackers_model: Option<Rc<TrackersModel>>,
    trackers_view: Option<Rc<TrackersView>>,
    last_json: Option<Rc<Obj>>,
    update_handler: Option<glib::SignalHandlerId>,
}

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-torrent-props-dialog.ui")]
    pub struct TorrentPropsDialog {
        #[template_child]
        pub notebook: TemplateChild<gtk::Notebook>,
        #[template_child]
        pub info_page: TemplateChild<gtk::Grid>,
        #[template_child]
        pub bandwidth_priority_combo: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub seed_ratio_mode_combo: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub seed_ratio_limit_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub queue_position_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub queue_position_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub honors_session_limits_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub download_limited_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub download_limit_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub upload_limited_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub upload_limit_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub peer_limit_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub sequential_download_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub sequential_from_piece_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub sequential_from_piece_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub size_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub have_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub dl_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub ul_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub state_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub date_started_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub eta_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub last_activity_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub error_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub destination_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub hash_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub privacy_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub origin_lb: TemplateChild<gtk::Label>,
        #[template_child]
        pub comment_buffer: TemplateChild<gtk::TextBuffer>,

        pub client: RefCell<Option<Client>>,
        pub win: glib::WeakRef<MainWindow>,
        pub target_ids: RefCell<Vec<i64>>,
        pub widgets: RefCell<JsonWidgets>,
        pub show_details: Cell<bool>,
        pub details: RefCell<Details>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TorrentPropsDialog {
        const NAME: &'static str = "TrgTorrentPropsDialog";
        type Type = super::TorrentPropsDialog;
        type ParentType = Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TorrentPropsDialog {
        fn dispose(&self) {
            let handler = self.details.borrow_mut().update_handler.take();
            if let (Some(h), Some(model)) =
                (handler, self.client.borrow().as_ref().and_then(|c| c.torrent_model()))
            {
                model.disconnect(h);
            }
        }
    }

    impl WidgetImpl for TorrentPropsDialog {}
    impl WindowImpl for TorrentPropsDialog {}
    impl DialogImpl for TorrentPropsDialog {}
}

glib::wrapper! {
    pub struct TorrentPropsDialog(ObjectSubclass<imp::TorrentPropsDialog>)
        @extends Dialog, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root,
            gtk::ShortcutManager;
}

#[gtk::template_callbacks]
impl TorrentPropsDialog {
    /// The ratio limit is for "Stop seeding at ratio"
    #[template_callback]
    fn seed_ratio_mode_changed_cb(dd: &gtk::DropDown, _pspec: &glib::ParamSpec) {
        if let Some(d) = dialog::ancestor::<TorrentPropsDialog>(dd) {
            d.imp().seed_ratio_limit_spin.set_sensitive(dd.selected() == 1);
        }
    }

    /// The first piece is for sequential downloads
    #[template_callback]
    fn sequential_download_toggled_cb(check: &gtk::CheckButton) {
        if let Some(d) = dialog::ancestor::<TorrentPropsDialog>(check) {
            d.imp().sequential_from_piece_spin.set_sensitive(check.is_active());
        }
    }

    pub fn new(win: &MainWindow, tv: &TreeView, client: &Client) -> Self {
        let d: Self = glib::Object::new();
        let imp = d.imp();
        *imp.client.borrow_mut() = Some(client.clone());
        imp.win.set(Some(win));

        let row_count = tv.count_selected();
        let prefs = client.prefs().clone();
        let show_details = prefs.get_int(prefs::KEY_STYLE, prefs::GLOBAL) != prefs::STYLE_CLASSIC
            && row_count == 1;
        imp.show_details.set(show_details);

        let data = client.torrent_data(win.selected_torrent_id());
        *imp.target_ids.borrow_mut() = torrent_view::selected_ids(tv);

        if row_count > 1 {
            d.set_title(Some(&sprintf(&tr("Multiple (%d) torrent properties"), &[&row_count])));
        } else if let Some((t, _)) = &data {
            d.set_title(Some(p::torrent_get_name(t)));
        }

        d.set_transient_for(Some(win));
        d.on_response(|d, response| d.response_cb(response));

        let empty = Rc::new(Obj::new());
        let (json, item) = match &data {
            Some((t, i)) => (t.clone(), Some(i.clone())),
            None => (empty, None),
        };

        let notebook = &imp.notebook;
        if show_details {
            let serial = client.serial();
            if let Some(item) = &item {
                d.info_page_update(&json, item);
            }

            let mut details = imp.details.borrow_mut();

            // Files
            let files_model = FilesModel::new();
            let files_view =
                FilesView::new(&files_model, win, client, Some("TrgFilesTreeView-dialog"));
            files_model.update(&json, p::TORRENT_GET_MODE_FIRST);
            notebook.insert_page(
                &files_view.tv,
                Some(&gtk::Label::new(Some(&tr("Files")))),
                Some(1),
            );

            // Peers
            let peers_model = PeersModel::new();
            let peers_tv =
                peers::peers_tree_view_new(&prefs, &peers_model, Some("TrgPeersTreeView-dialog"));
            peers_model.update(&peers_tv, serial, &json, p::TORRENT_GET_MODE_FIRST);
            notebook.insert_page(&peers_tv, Some(&gtk::Label::new(Some(&tr("Peers")))), Some(2));

            // Trackers
            let trackers_model = TrackersModel::new();
            let trackers_view =
                TrackersView::new(&trackers_model, client, win, Some("TrgTrackersTreeView-dialog"));
            trackers_model.update(serial, &json, p::TORRENT_GET_MODE_FIRST);
            notebook.insert_page(
                &trackers_view.tv,
                Some(&gtk::Label::new(Some(&tr("Trackers")))),
                Some(3),
            );

            details.files_model = Some(files_model);
            details.files_view = Some(files_view);
            details.peers_model = Some(peers_model);
            details.peers_tv = Some(peers_tv);
            details.trackers_model = Some(trackers_model);
            details.trackers_view = Some(trackers_view);
            details.last_json = Some(json.clone());

            if let Some(model) = client.torrent_model() {
                let weak = d.downgrade();
                details.update_handler = Some(model.connect_update(move || {
                    if let Some(d) = weak.upgrade() {
                        d.models_updated();
                    }
                }));
            }
        } else if let Some(n) = notebook.page_num(&imp.info_page.get()) {
            notebook.remove_page(Some(n));
        }

        d.limits_page_load(&json);

        let (width, height) = if show_details {
            let w = prefs.get_int("dialog-width", prefs::GLOBAL);
            let h = prefs.get_int("dialog-height", prefs::GLOBAL);
            if w <= 0 || h <= 0 { (700, 600) } else { (w, h) }
        } else {
            (500, 500)
        };
        d.set_default_size(width as i32, height as i32);

        d
    }

    fn client(&self) -> Client {
        self.imp().client.borrow().clone().unwrap()
    }

    fn response_cb(&self, response: i32) {
        let imp = self.imp();

        if imp.show_details.get() {
            let prefs = self.client().prefs().clone();
            let (w, h) = self.default_size();
            prefs.set_int("dialog-width", w as i64, prefs::GLOBAL);
            prefs.set_int("dialog-height", h as i64, prefs::GLOBAL);
            let details = imp.details.borrow();
            let flags = PERSIST_SORT | PERSIST_LAYOUT;
            if let Some(tv) = &details.peers_tv {
                tv.persist(flags);
            }
            if let Some(v) = &details.files_view {
                v.tv.persist(flags);
            }
            if let Some(v) = &details.trackers_view {
                v.tv.persist(flags);
            }
        }

        if response == dialog::RESPONSE_OK {
            let mut req = p::torrent_set(&imp.target_ids.borrow())
                .with(p::FIELD_SEED_RATIO_MODE, imp.seed_ratio_mode_combo.selected())
                .with(
                    p::FIELD_BANDWIDTH_PRIORITY,
                    imp.bandwidth_priority_combo.selected() as i64 - 1,
                );
            imp.widgets.borrow().save(req.params_mut());

            if let Some(win) = imp.win.upgrade() {
                win.dispatch_interactive(req);
            }
        }

        self.destroy();
    }

    fn info_page_update(&self, t: &Obj, item: &Item) {
        let imp = self.imp();
        let flags = item.uint(tm::COL_FLAGS);
        let error = item.int64(tm::COL_ERROR);

        imp.privacy_lb.set_text(&if get_bool(t, p::FIELD_ISPRIVATE) {
            tr("Private to this tracker -- DHT and PEX disabled")
        } else {
            tr("Public torrent")
        });

        let creator = get_str(t, p::FIELD_CREATOR).unwrap_or("");
        let date_created = get_i64(t, p::FIELD_DATE_CREATED);
        let date = util::epoch_to_string(date_created);
        let origin = if !creator.is_empty() && date_created > 0 {
            sprintf(&tr("Created by %1$s on %2$s"), &[&creator, &date])
        } else if date_created > 0 {
            sprintf(&tr("Created on %1$s"), &[&date])
        } else if !creator.is_empty() {
            sprintf(&tr("Created by %1$s"), &[&creator])
        } else {
            tr("N/A")
        };
        imp.origin_lb.set_text(&origin);

        imp.comment_buffer.set_text(get_str(t, p::FIELD_COMMENT).unwrap_or(""));
        imp.destination_lb.set_text(get_str(t, p::FIELD_DOWNLOAD_DIR).unwrap_or(""));
        imp.state_lb.set_text(&item.string(tm::COL_STATUS).unwrap_or_default());
        imp.date_started_lb.set_text(&util::epoch_to_string(get_i64(t, p::FIELD_ADDED_DATE)));

        let eta = item.int64(tm::COL_ETA);
        imp.eta_lb.set_text(&if eta > 0 { util::format_time_long(eta) } else { String::new() });

        imp.hash_lb.set_text(get_str(t, p::FIELD_HASH_STRING).unwrap_or(""));
        imp.error_lb.set_text(&if error != 0 {
            get_str(t, p::FIELD_ERROR_STRING).unwrap_or("").to_string()
        } else {
            tr("No errors")
        });

        if flags & p::TORRENT_FLAG_ACTIVE != 0 {
            imp.last_activity_lb.set_text(&tr("Active now"));
        } else {
            imp.last_activity_lb.set_text(&util::epoch_to_string(item.int64(tm::COL_LASTACTIVE)));
        }

        imp.size_lb.set_text(&util::format_size_or_none(item.int64(tm::COL_SIZEWHENDONE)));
        imp.dl_lb.set_text(&util::format_size_or_none(item.int64(tm::COL_DOWNLOADED)));
        imp.ul_lb.set_text(&util::format_size_or_none(item.int64(tm::COL_UPLOADED)));
        imp.have_lb.set_text(&util::format_size_or_none(item.int64(tm::COL_HAVE_VALID)));
    }

    fn limits_page_load(&self, json: &Obj) {
        let imp = self.imp();
        let mut widgets = imp.widgets.borrow_mut();

        let fields: [(&gtk::Widget, &str); 7] = [
            (imp.honors_session_limits_check.upcast_ref(), p::FIELD_HONORS_SESSION_LIMITS),
            (imp.download_limited_check.upcast_ref(), p::FIELD_DOWNLOAD_LIMITED),
            (imp.download_limit_spin.upcast_ref(), p::FIELD_DOWNLOAD_LIMIT),
            (imp.upload_limited_check.upcast_ref(), p::FIELD_UPLOAD_LIMITED),
            (imp.upload_limit_spin.upcast_ref(), p::FIELD_UPLOAD_LIMIT),
            (imp.seed_ratio_limit_spin.upcast_ref(), p::FIELD_SEED_RATIO_LIMIT),
            (imp.peer_limit_spin.upcast_ref(), p::FIELD_PEER_LIMIT),
        ];
        for (w, key) in fields {
            widgets.add(w, json, key);
        }

        if json.contains_key(p::FIELD_QUEUE_POSITION) {
            widgets.add(&imp.queue_position_spin.get(), json, p::FIELD_QUEUE_POSITION);
        } else {
            imp.queue_position_label.set_visible(false);
            imp.queue_position_spin.set_visible(false);
        }

        if json.contains_key(p::FIELD_SEQUENTIAL_DOWNLOAD) {
            widgets.add(&imp.sequential_download_check.get(), json, p::FIELD_SEQUENTIAL_DOWNLOAD);
            // the pieces of one torrent
            let pieces = get_i64(json, p::FIELD_PIECE_COUNT);
            if imp.target_ids.borrow().len() == 1 && pieces > 0 {
                imp.sequential_from_piece_spin.set_range(0.0, (pieces - 1) as f64);
                widgets.add(
                    &imp.sequential_from_piece_spin.get(),
                    json,
                    p::FIELD_SEQUENTIAL_DOWNLOAD_FROM_PIECE,
                );
                imp.sequential_from_piece_spin
                    .set_sensitive(imp.sequential_download_check.is_active());
            } else {
                imp.sequential_from_piece_label.set_visible(false);
                imp.sequential_from_piece_spin.set_visible(false);
            }
        } else {
            imp.sequential_download_check.set_visible(false);
            imp.sequential_from_piece_label.set_visible(false);
            imp.sequential_from_piece_spin.set_visible(false);
        }

        imp.bandwidth_priority_combo
            .set_selected((get_i64(json, p::FIELD_BANDWIDTH_PRIORITY) + 1).max(0) as u32);
        imp.seed_ratio_mode_combo
            .set_selected(get_i64(json, p::FIELD_SEED_RATIO_MODE).max(0) as u32);
        imp.seed_ratio_limit_spin.set_sensitive(imp.seed_ratio_mode_combo.selected() == 1);
    }

    fn models_updated(&self) {
        let imp = self.imp();
        let client = self.client();
        let serial = client.serial();
        let id = imp.target_ids.borrow().first().copied().unwrap_or(-1);
        let data = client.torrent_data(id);
        let exists = data.is_some();

        let (files_model, peers_model, peers_tv, trackers_model, last) = {
            let d = imp.details.borrow();
            (
                d.files_model.clone(),
                d.peers_model.clone(),
                d.peers_tv.clone(),
                d.trackers_model.clone(),
                d.last_json.clone(),
            )
        };

        if let Some((t, item)) = &data {
            if !last.as_ref().is_some_and(|l| Rc::ptr_eq(l, t)) {
                if let Some(m) = &files_model {
                    m.update(t, p::TORRENT_GET_MODE_UPDATE);
                }
                if let (Some(m), Some(tv)) = (&peers_model, &peers_tv) {
                    m.update(tv, serial, t, p::TORRENT_GET_MODE_UPDATE);
                }
                if let Some(m) = &trackers_model {
                    m.update(serial, t, p::TORRENT_GET_MODE_UPDATE);
                }
                self.info_page_update(t, item);
            }
        }

        let mut d = imp.details.borrow_mut();
        if let Some(v) = &d.files_view {
            v.tv.set_sensitive(exists);
        }
        if let Some(v) = &d.trackers_view {
            v.tv.set_sensitive(exists);
        }
        if let Some(tv) = &d.peers_tv {
            tv.set_sensitive(exists);
        }
        d.last_json = data.map(|(t, _)| t);
    }
}
