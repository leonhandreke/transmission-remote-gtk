//! The properties of the selected torrents.
//!
//! The torrent list is passed into here, which this gets the selection from.
//! If there are multiple selections, the first populates the fields. The IDs
//! are taken when the dialog is created, in case the selection changes before
//! clicking OK. The limits page's widgets are bound to a settings object
//! holding that torrent; on OK, the settings that changed are sent with the
//! IDs, so with several torrents, those of the others that weren't changed
//! stay as they are.
//!
//! In the Transmission styles, a single torrent also has pages of
//! information, files, peers and trackers, which keep themselves up to date.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::client::Client;
use crate::details::DetailLists;
use crate::dialogs::dialog::{Dialog, DialogImpl, Response, TrgDialogExt};
use crate::dialogs::settings::settings_object;
use crate::format;
use crate::i18n::{sprintf, tr};
use crate::main_window::MainWindow;
use crate::prefs::{self, Scope};
use crate::protocol::{self as p, Torrent};
use crate::torrent_model::UpdateMode;
use crate::torrent_object::{TorrentObject, TorrentValues};

settings_object! {
    /// The settings of the limits page, the fields of a torrent
    pub struct TorrentSettings("TrgTorrentSettings", Torrent) in mod settings_imp {
        honors_session_limits: bool,
        bandwidth_priority: i64,
        queue_position: Option<i64>,
        download_limited: bool,
        download_limit: i64,
        upload_limited: bool,
        upload_limit: i64,
        seed_ratio_mode: i64,
        seed_ratio_limit: f64,
        peer_limit: i64,
        sequential_download: Option<bool>,
        sequential_download_from_piece: i64,
    }
}

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/torrent-props-dialog.ui")]
    pub struct TorrentPropsDialog {
        #[template_child]
        pub notebook: TemplateChild<gtk::Notebook>,
        #[template_child]
        pub info_page: TemplateChild<gtk::Grid>,
        #[template_child]
        pub settings: TemplateChild<TorrentSettings>,
        #[template_child]
        pub bandwidth_priority_combo: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub queue_position_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub queue_position_spin: TemplateChild<gtk::SpinButton>,
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
        pub added_lb: TemplateChild<gtk::Label>,
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
        pub show_details: Cell<bool>,
        /// With show_details: the lists, the torrent shown and the handler
        /// updating them
        pub lists: OnceCell<DetailLists>,
        pub last_torrent: RefCell<Option<Rc<Torrent>>>,
        pub update_handler: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TorrentPropsDialog {
        const NAME: &'static str = "TrgTorrentPropsDialog";
        type Type = super::TorrentPropsDialog;
        type ParentType = Dialog;

        fn class_init(klass: &mut Self::Class) {
            TorrentSettings::ensure_type();
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TorrentPropsDialog {
        fn dispose(&self) {
            let handler = self.update_handler.take();
            if let (Some(h), Some(model)) =
                (handler, self.client.borrow().as_ref().map(|c| c.torrent_model()))
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
    #[template_callback(function)]
    fn stops_at_ratio(seed_ratio_mode: i64) -> bool {
        seed_ratio_mode == 1
    }

    /// The properties of the torrents ids, the selected ones
    pub fn new(win: &MainWindow, ids: Vec<i64>, client: &Client) -> Self {
        let d: Self = glib::Object::new();
        let imp = d.imp();
        *imp.client.borrow_mut() = Some(client.clone());
        imp.win.set(Some(win));

        let row_count = ids.len();
        let prefs = client.prefs().clone();
        let show_details = prefs.get_int(prefs::KEY_STYLE, Scope::Global) != prefs::STYLE_CLASSIC
            && row_count == 1;
        imp.show_details.set(show_details);

        let data = client.find_torrent(win.selected_torrent_id());
        *imp.target_ids.borrow_mut() = ids;

        if row_count > 1 {
            d.set_title(Some(&sprintf(&tr("Multiple (%d) torrent properties"), &[&row_count])));
        } else if let Some(obj) = &data {
            d.set_title(Some(&obj.torrent().name));
        }

        d.set_transient_for(Some(win));
        d.on_response(|d, response| d.response_cb(response));

        let torrent = data.as_ref().map_or_else(|| Rc::new(Torrent::default()), |o| o.torrent());

        let notebook = &imp.notebook;
        if show_details {
            let serial = client.serial();
            if let Some(obj) = &data {
                d.info_page_update(&torrent, &obj.values());
            }

            // after the information page
            let lists = DetailLists::new(client, "-dialog");
            lists.update(&torrent, serial, UpdateMode::First);
            let [trackers, files, peers] = lists.pages();
            for (position, (tv, title)) in [files, peers, trackers].into_iter().enumerate() {
                let label = gtk::Label::new(Some(&title));
                notebook.insert_page(tv, Some(&label), Some(position as u32 + 1));
            }
            imp.lists.set(lists).ok();
            imp.last_torrent.replace(Some(torrent.clone()));

            let weak = d.downgrade();
            let handler = client.torrent_model().connect_update(move || {
                if let Some(d) = weak.upgrade() {
                    d.models_updated();
                }
            });
            imp.update_handler.replace(Some(handler));
        } else if let Some(n) = notebook.page_num(&imp.info_page.get()) {
            notebook.remove_page(Some(n));
        }

        d.limits_page_load(&torrent);

        let (width, height) = if show_details {
            let w = prefs.get_int(prefs::KEY_PROPS_DIALOG_WIDTH, Scope::Global);
            let h = prefs.get_int(prefs::KEY_PROPS_DIALOG_HEIGHT, Scope::Global);
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

    fn response_cb(&self, response: Response) {
        let imp = self.imp();

        if imp.show_details.get() {
            let prefs = self.client().prefs().clone();
            let (w, h) = self.default_size();
            prefs.set_int(prefs::KEY_PROPS_DIALOG_WIDTH, w as i64, Scope::Global);
            prefs.set_int(prefs::KEY_PROPS_DIALOG_HEIGHT, h as i64, Scope::Global);
            if let Some(lists) = imp.lists.get() {
                lists.persist();
            }
        }

        // the settings that changed, if any
        if response == Response::Ok {
            let changes = imp.settings.changes();
            if !changes.is_empty() {
                let mut req = p::torrent_set(&imp.target_ids.borrow());
                req.params_mut().extend(changes);
                self.client().dispatch_interactive(req);
            }
        }

        self.destroy();
    }

    fn info_page_update(&self, t: &Torrent, v: &TorrentValues) {
        let imp = self.imp();

        imp.privacy_lb.set_text(&if t.is_private {
            tr("Private to this tracker -- DHT and PEX disabled")
        } else {
            tr("Public torrent")
        });

        let creator = &t.creator;
        let date = format::date_time(t.date_created);
        let origin = if !creator.is_empty() && t.date_created > 0 {
            sprintf(&tr("Created by %1$s on %2$s"), &[creator, &date])
        } else if t.date_created > 0 {
            sprintf(&tr("Created on %1$s"), &[&date])
        } else if !creator.is_empty() {
            sprintf(&tr("Created by %1$s"), &[creator])
        } else {
            tr("N/A")
        };
        imp.origin_lb.set_text(&origin);

        imp.comment_buffer.set_text(&t.comment);
        imp.destination_lb.set_text(&t.download_dir);
        imp.state_lb.set_text(&v.status);
        imp.added_lb.set_text(&format::date_time(t.added_date));

        // as in the general panel
        let eta = t.eta;
        imp.eta_lb.set_text(&if eta > 0 { format::time_long(eta) } else { tr("N/A") });

        imp.hash_lb.set_text(&t.hash_string);
        imp.error_lb.set_text(&if t.error != 0 { t.error_string.clone() } else { tr("No errors") });

        // Transferring now, or when it last did (the activity date)
        if t.rate_download > 0 || t.rate_upload > 0 {
            imp.last_activity_lb.set_text(&tr("Active now"));
        } else {
            imp.last_activity_lb.set_text(&format::date_time(t.activity_date));
        }

        imp.size_lb.set_text(&format::size_or_none(t.size_when_done));
        imp.dl_lb.set_text(&format::size_or_none(t.downloaded_ever));
        imp.ul_lb.set_text(&format::size_or_none(t.uploaded_ever));
        imp.have_lb.set_text(&format::size_or_none(t.have_valid));
    }

    fn limits_page_load(&self, torrent: &Torrent) {
        let imp = self.imp();

        let has_queue = torrent.queue_position.is_some();
        imp.queue_position_label.set_visible(has_queue);
        imp.queue_position_spin.set_visible(has_queue);

        // the first piece of one torrent
        let has_sequential = torrent.sequential_download.is_some();
        let pieces = torrent.piece_count;
        let has_from_piece = has_sequential && imp.target_ids.borrow().len() == 1 && pieces > 0;
        imp.sequential_download_check.set_visible(has_sequential);
        imp.sequential_from_piece_label.set_visible(has_from_piece);
        imp.sequential_from_piece_spin.set_visible(has_from_piece);
        if has_from_piece {
            imp.sequential_from_piece_spin.set_range(0.0, (pieces - 1) as f64);
        }

        imp.settings.load(torrent.clone());

        // TR_PRI_LOW to TR_PRI_HIGH, in the order of the drop-down
        imp.settings
            .bind_property("bandwidth-priority", &*imp.bandwidth_priority_combo, "selected")
            .transform_to(|_, priority: i64| Some((priority - p::TR_PRI_LOW).max(0) as u32))
            .transform_from(|_, selected: u32| Some(selected as i64 + p::TR_PRI_LOW))
            .bidirectional()
            .sync_create()
            .build();
    }

    fn models_updated(&self) {
        let imp = self.imp();
        let client = self.client();
        let serial = client.serial();
        let id = imp.target_ids.borrow().first().copied().unwrap_or(-1);
        let data = client.find_torrent(id);
        let exists = data.is_some();

        let Some(lists) = imp.lists.get() else { return };

        let torrent = data.as_ref().map(TorrentObject::torrent);
        let last = imp.last_torrent.replace(torrent.clone());
        if let (Some(obj), Some(t)) = (&data, &torrent)
            && !last.is_some_and(|l| Rc::ptr_eq(&l, t))
        {
            lists.update(t, serial, UpdateMode::Update);
            self.info_page_update(t, &obj.values());
        }
        lists.set_sensitive(exists);
    }
}
