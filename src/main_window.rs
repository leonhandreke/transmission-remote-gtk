//! The rather large main window, which glues everything together
//! (trg-main-window.c).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, gdk, gio, glib};

use crate::client::{Client, Response};
use crate::dialog;
use crate::dialogs;
use crate::files::{FilesModel, FilesView};
use crate::i18n::{sprintf, tr};
use crate::item::Item;
use crate::menus::{MenuBar, Toolbar};
use crate::panels::{GeneralPanel, StatusBar};
use crate::peers::{self, PeersModel};
use crate::prefs;
use crate::protocol::{self as p, Obj, get_bool, get_i64, get_str};
use crate::state_selector::{FILTER_FLAG_DIR, FILTER_FLAG_TRACKER, StateSelector};
use crate::torrent_model::{self as tm, TorrentItemExt, TorrentModel};
use crate::torrent_view;
use crate::trackers::{TrackersModel, TrackersView};
use crate::tree_view::{PERSIST_LAYOUT, PERSIST_SORT, TreeView};
use crate::upload::{self, Upload};
use crate::util;

#[derive(Default)]
pub struct Parts {
    pub client: Option<Client>,
    pub torrent_model: Option<TorrentModel>,
    pub torrent_tv: Option<TreeView>,
    pub torrent_filter: Option<gtk::CustomFilter>,
    pub toolbar: Option<Toolbar>,
    pub menu_bar: Option<Rc<MenuBar>>,
    pub state_selector: Option<StateSelector>,
    pub trackers_model: Option<Rc<TrackersModel>>,
    pub trackers_view: Option<Rc<TrackersView>>,
    pub files_model: Option<Rc<FilesModel>>,
    pub files_view: Option<Rc<FilesView>>,
    pub peers_model: Option<Rc<PeersModel>>,
    pub peers_tv: Option<TreeView>,
}

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-main-window.ui")]
    pub struct MainWindow {
        #[template_child]
        pub menu_bar: TemplateChild<gtk::PopoverMenuBar>,
        #[template_child]
        pub toolbar_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub hpaned: TemplateChild<gtk::Paned>,
        #[template_child]
        pub vpaned: TemplateChild<gtk::Paned>,
        #[template_child]
        pub notebook: TemplateChild<gtk::Notebook>,
        #[template_child]
        pub torrent_actions_menu: TemplateChild<gio::MenuModel>,
        #[template_child]
        pub torrent_queue_menu: TemplateChild<gio::MenuModel>,
        #[template_child]
        pub filter_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub gen_details: TemplateChild<GeneralPanel>,
        #[template_child]
        pub status_bar: TemplateChild<StatusBar>,

        pub parts: RefCell<Parts>,
        pub selected_torrent_id: Cell<i64>,

        // what the torrent filter shows, from the state selector and the entry
        pub filter_criteria: Cell<u32>,
        pub filter_selected_text: RefCell<Option<String>>,
        pub filter_text: RefCell<Option<String>>,

        pub timer_id: RefCell<Option<glib::SourceId>>,
        pub session_timer_id: RefCell<Option<glib::SourceId>>,
        pub queues_enabled: Cell<bool>,
        pub state_saved: Cell<bool>,
        pub args: RefCell<Option<Vec<String>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MainWindow {
        const NAME: &'static str = "TrgMainWindow";
        type Type = super::MainWindow;
        type ParentType = gtk::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            GeneralPanel::ensure_type();
            StatusBar::ensure_type();
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MainWindow {}
    impl WidgetImpl for MainWindow {}
    impl WindowImpl for MainWindow {}
    impl ApplicationWindowImpl for MainWindow {}
}

glib::wrapper! {
    pub struct MainWindow(ObjectSubclass<imp::MainWindow>)
        @extends gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
            gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

fn main_window_of(w: &gtk::Widget) -> Option<MainWindow> {
    dialog::ancestor::<MainWindow>(w)
}

#[gtk::template_callbacks]
impl MainWindow {
    #[template_callback]
    fn close_request(win: &gtk::Window) -> bool {
        if let Some(win) = win.downcast_ref::<MainWindow>() {
            win.save_window_state();
        }
        false
    }

    #[template_callback]
    fn destroy_window(_win: &gtk::Widget) {
        if let Some(app) = gio::Application::default() {
            app.quit();
        }
    }

    #[template_callback]
    fn window_key_press_handler(
        controller: &gtk::EventControllerKey,
        keyval: u32,
        _keycode: u32,
        state: gdk::ModifierType,
    ) -> bool {
        let Some(win) = controller.widget().and_then(|w| main_window_of(&w)) else {
            return false;
        };
        let key: gdk::Key = unsafe { glib::translate::from_glib(keyval) };
        if state.contains(util::primary_modifier()) && (key == gdk::Key::k || key == gdk::Key::f) {
            win.imp().filter_entry.grab_focus();
            return true;
        }
        false
    }

    #[template_callback]
    fn entry_filter_changed_cb(entry: &gtk::Entry) {
        if let Some(win) = main_window_of(entry.upcast_ref()) {
            win.refilter();
            entry.set_icon_sensitive(gtk::EntryIconPosition::Secondary, entry.text_length() > 0);
        }
    }

    #[template_callback]
    fn clear_filter_entry_cb(entry: &gtk::Entry, _pos: gtk::EntryIconPosition) {
        entry.set_text("");
    }
}

impl MainWindow {
    pub fn new(app: &gtk::Application, client: &Client) -> Self {
        let win: Self = glib::Object::builder().property("application", app).build();
        win.imp().parts.borrow_mut().client = Some(client.clone());
        win.construct();
        win
    }

    pub fn client(&self) -> Client {
        self.imp().parts.borrow().client.clone().unwrap()
    }

    fn torrent_tv(&self) -> TreeView {
        self.imp().parts.borrow().torrent_tv.clone().unwrap()
    }

    fn torrent_model(&self) -> TorrentModel {
        self.imp().parts.borrow().torrent_model.clone().unwrap()
    }

    fn menu_bar(&self) -> Rc<MenuBar> {
        self.imp().parts.borrow().menu_bar.clone().unwrap()
    }

    pub fn state_selector(&self) -> StateSelector {
        self.imp().parts.borrow().state_selector.clone().unwrap()
    }

    pub fn status_bar(&self) -> StatusBar {
        self.imp().status_bar.get()
    }

    pub fn selected_torrent_id(&self) -> i64 {
        self.imp().selected_torrent_id.get()
    }

    pub fn menu_model(&self) -> gio::MenuModel {
        self.menu_bar().model()
    }

    fn reset_connect_args(&self) {
        self.imp().args.take();
    }

    fn update_selected_torrent_notebook(&self, mode: i32, id: i64) {
        let client = self.client();
        let serial = client.serial();
        let data = if id >= 0 { client.torrent_data(id) } else { None };

        match data {
            Some((t, item)) => {
                let parts = self.imp().parts.borrow();
                self.menu_bar().torrent_actions_sensitive(true);
                self.imp().gen_details.update(&t, &item);
                parts.trackers_model.as_ref().unwrap().update(serial, &t, mode);
                parts.files_model.as_ref().unwrap().update(&t, mode);
                parts.peers_model.as_ref().unwrap().update(
                    parts.peers_tv.as_ref().unwrap(),
                    serial,
                    &t,
                    mode,
                );
            }
            None => self.torrent_scrub(),
        }

        self.imp().selected_torrent_id.set(id);
    }

    fn torrent_event_notification(&self, icon_name: &str, desc: &str, pref_key: &str, item: &Item) {
        if !self.client().prefs().get_bool(pref_key, prefs::NOFLAGS) {
            return;
        }
        let name = item.string(tm::COL_NAME).unwrap_or_default();
        let notification = gio::Notification::new(&name);
        notification.set_body(Some(desc));
        notification.set_icon(&gio::ThemedIcon::new(icon_name));
        notification.set_priority(gio::NotificationPriority::Low);
        if let Some(app) = gio::Application::default() {
            app.send_notification(Some(&name), &notification);
        }
    }

    fn save_window_size(&self) {
        let prefs = self.client().prefs().clone();
        let (width, height) = self.default_size();
        prefs.set_int(prefs::KEY_WINDOW_HEIGHT, height as i64, prefs::GLOBAL);
        prefs.set_int(prefs::KEY_WINDOW_WIDTH, width as i64, prefs::GLOBAL);
    }

    /// Persists the window and tree view layout. This has to happen before
    /// the window is destroyed, as its children are already gone by
    /// "destroy" in GTK 4.
    pub fn save_window_state(&self) {
        let imp = self.imp();
        if imp.state_saved.get() {
            return;
        }
        let prefs = self.client().prefs().clone();

        self.save_window_size();
        prefs.set_int(prefs::KEY_NOTEBOOK_PANED_POS, imp.vpaned.position() as i64, prefs::GLOBAL);
        prefs.set_int(prefs::KEY_STATES_PANED_POS, imp.hpaned.position() as i64, prefs::GLOBAL);

        {
            let parts = imp.parts.borrow();
            let flags = PERSIST_SORT | PERSIST_LAYOUT;
            parts.peers_tv.as_ref().unwrap().persist(flags);
            parts.files_view.as_ref().unwrap().tv.persist(flags);
            parts.torrent_tv.as_ref().unwrap().persist(flags);
            parts.trackers_view.as_ref().unwrap().tv.persist(flags);
        }
        prefs.save();

        imp.state_saved.set(true);
    }

    fn open_props(&self) {
        if self.selected_torrent_id() < 0 {
            return;
        }
        dialogs::props::TorrentPropsDialog::new(self, &self.torrent_tv(), &self.client()).present();
    }

    fn copy_magnet_link(&self) {
        let id = self.selected_torrent_id();
        if id < 0 {
            return;
        }
        if let Some((t, _)) = self.client().torrent_data(id) {
            self.clipboard().set_text(get_str(&t, p::FIELD_MAGNETLINK).unwrap_or(""));
        }
    }

    fn selected_ids(&self) -> Vec<i64> {
        torrent_view::selected_ids(&self.torrent_tv())
    }

    /// Sends a request about torrents, and in the same batch, which the
    /// server executes in order, gets them again (all of them if it's about
    /// all of them, the recently active ones and removed IDs for a
    /// removal). Shows its error.
    pub fn dispatch_interactive(&self, req: p::Request) {
        self.dispatch_interactive_with(req, |_| {});
    }

    /// dispatch_interactive(), after() gets the result of the request first
    pub fn dispatch_interactive_with(
        &self,
        req: p::Request,
        after: impl FnOnce(&Response) + 'static,
    ) {
        let refresh = if req.method == p::METHOD_TORRENT_REMOVE {
            p::Torrents::RecentlyActive
        } else {
            p::Torrents::of(&req)
        };
        let weak = self.downgrade();
        self.client().dispatch_batch(vec![req, p::torrent_get(&refresh)], move |mut results| {
            let Some(win) = weak.upgrade() else { return };
            let (Some(torrents), Some(result)) = (results.pop(), results.pop()) else { return };
            after(&result);
            if !win.client().is_connected() {
                return;
            }
            win.dialog_error_handler(&result);
            win.on_torrent_get(torrents, p::TORRENT_GET_MODE_INTERACTION);
        });
    }

    /// Sends a session_set, and in the same batch gets the session again
    pub fn dispatch_session_set(&self, req: p::Request) {
        let weak = self.downgrade();
        self.client().update_session_after(vec![req], move |results, session| {
            let Some(win) = weak.upgrade() else { return };
            if let Some(result) = results.first() {
                win.dialog_error_handler(result);
            }
            win.on_session_get(session);
        });
    }

    /// Gets torrents (after they changed outside of a batch, such as adding
    /// them)
    pub fn refresh_torrents(&self, torrents: &p::Torrents) {
        let weak = self.downgrade();
        self.client().dispatch(p::torrent_get(torrents), move |r| {
            if let Some(win) = weak.upgrade() {
                win.on_torrent_get(r, p::TORRENT_GET_MODE_INTERACTION);
            }
        });
    }

    /// Adds torrents (files, URLs or magnet links), with the add dialog if
    /// that's what the preferences say
    pub fn add_from_filenames(&self, uris: Vec<String>) {
        let client = self.client();
        if !client.is_connected() || uris.is_empty() {
            return;
        }
        let prefs = client.prefs();
        if prefs.get_bool(prefs::KEY_ADD_OPTIONS_DIALOG, prefs::GLOBAL) {
            dialogs::add::TorrentAddDialog::new_from_filenames(self, &client, uris).present();
        } else {
            upload::do_upload(Upload::new(uris, &client, self, prefs.add_flags()));
        }
    }

    fn disconnect(&self) {
        self.client().inc_connid();
        self.conn_changed(false);
        self.imp().status_bar.reset();
    }

    /// parameter is the index of the profile to connect to, or -1 for the
    /// current one
    pub fn connect_profile(&self, index: i32) {
        let client = self.client();
        let prefs = client.prefs().clone();
        let current = prefs.profile();
        let profile =
            (index >= 0 && (index as usize) < prefs.n_profiles()).then_some(index as usize);

        if client.is_connected() {
            self.disconnect();
        }

        match profile {
            Some(p) if Some(p) != current => prefs.set_profile(Some(p)),
            _ => prefs.emit_profile_changed(),
        }

        if let Err(msg) = client.parse_settings() {
            self.reset_connect_args();
            // open the preferences once the (modal) error is dismissed
            let weak = self.downgrade();
            util::error_dialog_full(
                Some(self),
                &msg,
                Some(Box::new(move || {
                    if let Some(win) = weak.upgrade() {
                        win.open_local_prefs();
                    }
                })),
            );
            return;
        }

        self.imp().status_bar.push_connection_msg(&tr("Connecting..."));
        client.inc_connid();
        let weak = self.downgrade();
        // the session and the torrents in one go
        client.dispatch_batch(
            vec![p::session_get(), p::torrent_get(&p::Torrents::All)],
            move |mut results| {
                let Some(win) = weak.upgrade() else { return };
                let (Some(torrents), Some(session)) = (results.pop(), results.pop()) else {
                    return;
                };
                let was_connected = win.client().is_connected();
                win.on_session_get(session);
                if !was_connected && win.client().is_connected() {
                    win.on_torrent_get(torrents, p::TORRENT_GET_MODE_FIRST);
                    if let Some(args) = win.imp().args.take() {
                        win.add_from_filenames(args);
                    }
                }
            },
        );
    }

    fn open_local_prefs(&self) {
        dialogs::preferences::instance(self, &self.client()).present();
    }

    fn open_remote_prefs(&self) {
        if self.client().is_connected() {
            dialogs::remote_prefs::instance(self, &self.client()).present();
        }
    }

    fn is_ready_for_torrent_action(&self) -> bool {
        self.selected_torrent_id() >= 0 && self.client().is_connected()
    }

    /// Asks for confirmation and removes the torrents in ids
    fn confirm_action_dialog(
        &self,
        action_name: &str,
        action_label: &str,
        ids: Vec<i64>,
        delete_data: bool,
    ) {
        let tv = self.torrent_tv();
        let count = tv.count_selected();

        let markup = if count == 1 {
            let name = tv.first_selected().and_then(|i| i.string(tm::COL_NAME)).unwrap_or_default();
            sprintf(
                &tr("<big><b>%s \"%s\"?</b></big>"),
                &[&glib::markup_escape_text(action_name), &glib::markup_escape_text(&name)],
            )
        } else if count > 1 {
            sprintf(
                &tr("<big><b>%s %d torrents?</b></big>"),
                &[&glib::markup_escape_text(action_name), &count],
            )
        } else {
            return;
        };

        // The (translated) message is markup, an alert shows plain text
        let message = gtk::pango::parse_markup(&markup, '\0')
            .map(|(_, text, _)| text.to_string())
            .unwrap_or(markup);

        let dialog = gtk::AlertDialog::builder()
            .message(message)
            .buttons([tr("_Cancel"), action_label.to_string()])
            .cancel_button(0)
            .default_button(0)
            .modal(true)
            .build();

        let weak = self.downgrade();
        dialog.choose(Some(self), gio::Cancellable::NONE, move |result| {
            let Some(win) = weak.upgrade() else { return };
            if result != Ok(1) {
                return;
            }
            let req = p::torrent_remove(&ids, delete_data);
            let w = win.downgrade();
            win.dispatch_interactive_with(req, move |r| {
                // the free space changed
                if let (true, Ok(_), Some(win)) = (delete_data, r, w.upgrade()) {
                    let w = win.downgrade();
                    win.client().update_session(move |r| {
                        if let Some(win) = w.upgrade() {
                            win.on_session_get(r);
                        }
                    });
                }
            });
        });
    }

    fn notebook_fill(&self) {
        let client = self.client();
        let notebook = &self.imp().notebook;

        let trackers_model = TrackersModel::new();
        let trackers_view = TrackersView::new(&trackers_model, &client, self, None);
        notebook.append_page(&trackers_view.tv, Some(&gtk::Label::new(Some(&tr("Trackers")))));

        let files_model = FilesModel::new();
        let files_view = FilesView::new(&files_model, self, &client, None);
        notebook.append_page(&files_view.tv, Some(&gtk::Label::new(Some(&tr("Files")))));

        let peers_model = PeersModel::new();
        let peers_tv = peers::peers_tree_view_new(client.prefs(), &peers_model, None);
        notebook.append_page(&peers_tv, Some(&gtk::Label::new(Some(&tr("Peers")))));

        let mut parts = self.imp().parts.borrow_mut();
        parts.trackers_model = Some(trackers_model);
        parts.trackers_view = Some(trackers_view);
        parts.files_model = Some(files_model);
        parts.files_view = Some(files_view);
        parts.peers_model = Some(peers_model);
        parts.peers_tv = Some(peers_tv);
    }

    fn start_session_timer(&self) {
        let prefs = self.client().prefs().clone();
        let interval =
            prefs.get_int(prefs::KEY_SESSION_UPDATE_INTERVAL, prefs::CONNECTION).max(1) as u32;
        let weak = self.downgrade();
        let id = glib::timeout_add_seconds_local_once(interval, move || {
            if let Some(win) = weak.upgrade() {
                // a one-shot timer, so the id is no longer valid
                win.imp().session_timer_id.take();
                let w = win.downgrade();
                win.client().update_session(move |r| {
                    if let Some(win) = w.upgrade() {
                        win.on_session_get(r);
                        win.start_session_timer();
                    }
                });
            }
        });
        if let Some(old) = self.imp().session_timer_id.replace(Some(id)) {
            old.remove();
        }
    }

    fn start_torrents_timer(&self, interval: u32) {
        let weak = self.downgrade();
        let id = glib::timeout_add_seconds_local_once(interval, move || {
            if let Some(win) = weak.upgrade() {
                win.imp().timer_id.take();
                win.update_torrents();
            }
        });
        if let Some(old) = self.imp().timer_id.replace(Some(id)) {
            old.remove();
        }
    }

    fn update_torrents(&self) {
        let client = self.client();
        if !client.is_connected() {
            return;
        }
        let prefs = client.prefs();
        let every = prefs.get_int(prefs::ACTIVEONLY_FULLSYNC_EVERY, prefs::CONNECTION);
        let active_only = prefs.get_bool(prefs::KEY_UPDATE_ACTIVE_ONLY, prefs::CONNECTION)
            && (!prefs.get_bool(prefs::ACTIVEONLY_FULLSYNC_ENABLED, prefs::CONNECTION)
                || (every != 0 && client.serial() % every != 0));
        let (torrents, mode) = if active_only {
            (p::Torrents::RecentlyActive, p::TORRENT_GET_MODE_ACTIVE)
        } else {
            (p::Torrents::All, p::TORRENT_GET_MODE_UPDATE)
        };
        let weak = self.downgrade();
        client.dispatch(p::torrent_get(&torrents), move |r| {
            if let Some(win) = weak.upgrade() {
                win.on_torrent_get(r, mode);
            }
        });
    }

    fn on_session_get(&self, response: Response) {
        let client = self.client();
        let is_connected = client.is_connected();
        let last_session = client.session_clone();
        let new_session = response.as_ref().ok().cloned();

        // servers older than 4.1 fail with Error::Unsupported
        if !is_connected {
            if self.dialog_error_handler(&response) {
                self.reset_connect_args();
                return;
            }
            if let Some(s) = &new_session {
                self.imp().status_bar.connect(s, &client);
            }
        }

        if let Some(new_session) = new_session {
            let reload_aliases = last_session.as_ref().is_some_and(|l| {
                get_str(l, p::SGET_DOWNLOAD_DIR) != get_str(&new_session, p::SGET_DOWNLOAD_DIR)
            });
            let changed = |k| {
                last_session.as_ref().is_some_and(|l| get_bool(l, k) != get_bool(&new_session, k))
            };
            let refresh_speed = changed(p::SGET_ALT_SPEED_ENABLED)
                || changed(p::SGET_SPEED_LIMIT_DOWN_ENABLED)
                || changed(p::SGET_SPEED_LIMIT_UP_ENABLED);

            client.set_session(new_session);

            if reload_aliases {
                self.reload_dir_aliases();
            }
            if refresh_speed {
                self.imp().status_bar.update_speed(&self.torrent_model().stats(), &client);
            }
        }

        if !is_connected {
            self.conn_changed(true);
        }
    }

    /// The callback for a torrent_get response
    fn on_torrent_get(&self, response: Response, mode: i32) {
        let client = self.client();
        let prefs = client.prefs().clone();
        let imp = self.imp();

        // Disconnected between request and response callback
        if !client.is_connected() {
            return;
        }

        let mut interval = prefs.get_int(prefs::KEY_UPDATE_INTERVAL, prefs::CONNECTION);
        if interval < 1 {
            interval = prefs::INTERVAL_DEFAULT;
        }
        let interval = interval as u32;

        let obj = match response {
            Ok(obj) => obj,
            Err(error) => {
                self.on_torrent_get_failed(&error, interval);
                return;
            }
        };

        client.reset_fail_count();
        client.inc_serial();

        let stats = self.torrent_model().update(&client, &obj, mode);

        // The filter and sorter don't watch the items, so the values they
        // look at may have changed.
        if (imp.filter_criteria.get() != 0 || imp.filter_text.borrow().is_some())
            && let Some(f) = imp.parts.borrow().torrent_filter.as_ref()
        {
            f.changed(gtk::FilterChange::Different);
        }
        self.torrent_tv().resort();

        self.update_selected_torrent_notebook(mode, self.selected_torrent_id());
        imp.status_bar.update(&stats, &client);

        if mode != p::TORRENT_GET_MODE_INTERACTION {
            self.start_torrents_timer(interval);
        }
    }

    /// A failed torrent_get: retried with the next update, until there were
    /// too many failures, which disconnects
    fn on_torrent_get_failed(&self, error: &crate::client::Error, interval: u32) {
        let client = self.client();
        let prefs = client.prefs().clone();
        let max_retries = prefs.get_int(prefs::KEY_RETRIES, prefs::CONNECTION);
        if client.inc_fail_count() as i64 >= max_retries {
            self.conn_changed(false);
            self.show_error(&error.to_string());
        } else {
            let status_msg = sprintf(
                &tr("Request %d/%d failed: %s"),
                &[&client.fail_count(), &max_retries, &error.to_string()],
            );
            self.imp().status_bar.push_connection_msg(&status_msg);
            self.start_torrents_timer(interval);
        }
    }

    fn visible_func(&self, item: &Item) -> bool {
        let imp = self.imp();
        let criteria = imp.filter_criteria.get();

        if criteria != 0 {
            if criteria & FILTER_FLAG_TRACKER != 0 {
                let Some(json) = item.torrent_json() else { return false };
                let selector = self.state_selector();
                if !p::torrent_has_tracker(
                    &json,
                    selector.host_regex(),
                    imp.filter_selected_text.borrow().as_deref(),
                ) {
                    return false;
                }
            } else if criteria & FILTER_FLAG_DIR != 0 {
                if imp.filter_selected_text.borrow().as_deref()
                    != item.string(tm::COL_DOWNLOADDIR_SHORT).as_deref()
                {
                    return false;
                }
            } else if item.uint(tm::COL_FLAGS) & criteria == 0 {
                return false;
            }
        }

        if let Some(text) = imp.filter_text.borrow().as_ref()
            && let Some(name) = item.string(tm::COL_NAME)
        {
            return glib::casefold(&name).contains(text.as_str());
        }

        true
    }

    /// The state selector or filter entry changed
    fn refilter(&self) {
        let imp = self.imp();
        let Some(selector) = imp.parts.borrow().state_selector.clone() else { return };
        let text = imp.filter_entry.text();

        imp.filter_criteria.set(selector.flag());
        *imp.filter_selected_text.borrow_mut() =
            if selector.flag() & (FILTER_FLAG_TRACKER | FILTER_FLAG_DIR) != 0 {
                selector.selected_text()
            } else {
                None
            };
        *imp.filter_text.borrow_mut() =
            (!text.is_empty()).then(|| glib::casefold(&text).to_string());

        if let Some(f) = imp.parts.borrow().torrent_filter.as_ref() {
            f.changed(gtk::FilterChange::Different);
        }
    }

    pub fn reload_dir_aliases(&self) {
        self.torrent_model().reload_dir_aliases(&self.client());
    }

    /// Shows the error of a failed request. Returns whether it failed.
    fn dialog_error_handler(&self, response: &Response) -> bool {
        match response {
            Err(error) => {
                self.show_error(&error.to_string());
                true
            }
            Ok(_) => false,
        }
    }

    fn show_error(&self, msg: &str) {
        let sb = &self.imp().status_bar;
        sb.clear_indicators();
        sb.push_connection_msg(msg);
        util::error_dialog(Some(self), msg);
    }

    fn first_selected_id(&self) -> i64 {
        self.torrent_tv().first_selected().map(|i| i.torrent_id()).unwrap_or(-1)
    }

    fn torrent_scrub(&self) {
        let imp = self.imp();
        {
            let parts = imp.parts.borrow();
            parts.files_model.as_ref().unwrap().clear();
            let trackers = parts.trackers_model.as_ref().unwrap();
            trackers.clear();
            parts.peers_model.as_ref().unwrap().clear();
            trackers.set_no_selection();
        }
        imp.gen_details.clear();
        self.menu_bar().torrent_actions_sensitive(false);
    }

    fn conn_changed(&self, connected: bool) {
        let imp = self.imp();
        let client = self.client();

        self.menu_bar().connected_change(connected);

        {
            let parts = imp.parts.borrow();
            parts.torrent_tv.as_ref().unwrap().set_sensitive(connected);
            parts.peers_tv.as_ref().unwrap().set_sensitive(connected);
            parts.files_view.as_ref().unwrap().tv.set_sensitive(connected);
            parts.trackers_view.as_ref().unwrap().tv.set_sensitive(connected);
        }
        imp.gen_details.set_sensitive(connected);

        if connected {
            self.start_session_timer();
        } else {
            self.torrent_scrub();
            self.state_selector().disconnect();
            self.torrent_model().remove_all();
            if let Some(id) = imp.timer_id.take() {
                id.remove();
            }
            if let Some(id) = imp.session_timer_id.take() {
                id.remove();
            }
        }

        client.status_change(connected);
    }

    fn add_actions(&self) {
        let action = |name: &str, f: fn(&MainWindow)| {
            let a = gio::SimpleAction::new(name, None);
            let weak = self.downgrade();
            a.connect_activate(move |_, _| {
                if let Some(win) = weak.upgrade() {
                    f(&win);
                }
            });
            self.add_action(&a);
        };

        // parameter: profile index, -1 for the current profile. state: the
        // current profile
        let connect = gio::SimpleAction::new_stateful(
            "connect",
            Some(glib::VariantTy::INT32),
            &(-1i32).to_variant(),
        );
        let weak = self.downgrade();
        connect.connect_activate(move |_, param| {
            if let Some(win) = weak.upgrade() {
                win.connect_profile(param.and_then(|p| p.get::<i32>()).unwrap_or(-1));
            }
        });
        self.add_action(&connect);

        action("disconnect", |w| w.disconnect());
        action("add", |w| {
            if w.client().is_connected() {
                dialogs::add::add_dialog(w, &w.client());
            }
        });
        action("add-url", |w| dialogs::add_url::TorrentAddUrlDialog::new(w, &w.client()).present());
        action("properties", |w| w.open_props());
        action("copy-magnet", |w| w.copy_magnet_link());
        action("resume", |w| {
            if w.client().is_connected() {
                w.dispatch_interactive(p::torrent_start(Some(&w.selected_ids())));
            }
        });
        action("pause", |w| {
            if w.client().is_connected() {
                w.dispatch_interactive(p::torrent_pause(Some(&w.selected_ids())));
            }
        });
        action("verify", |w| {
            if w.is_ready_for_torrent_action() {
                w.dispatch_interactive(p::torrent_verify(&w.selected_ids()));
            }
        });
        action("reannounce", |w| {
            if w.client().is_connected() {
                w.dispatch_interactive(p::torrent_reannounce(&w.selected_ids()));
            }
        });
        action("move", |w| {
            if w.is_ready_for_torrent_action() {
                dialogs::move_dialog::TorrentMoveDialog::new(w, &w.client(), &w.torrent_tv())
                    .present();
            }
        });
        action("remove", |w| {
            if w.is_ready_for_torrent_action() {
                w.confirm_action_dialog(&tr("Remove"), &tr("_Remove"), w.selected_ids(), false);
            }
        });
        action("delete", |w| {
            if w.is_ready_for_torrent_action() {
                w.confirm_action_dialog(
                    &tr("Remove and delete"),
                    &tr("_Delete"),
                    w.selected_ids(),
                    true,
                );
            }
        });
        action("start-now", |w| {
            if w.is_ready_for_torrent_action() {
                w.dispatch_interactive(p::torrent_start_now(&w.selected_ids()));
            }
        });
        action("up-queue", |w| {
            if w.imp().queues_enabled.get() && w.is_ready_for_torrent_action() {
                w.dispatch_interactive(p::torrent_queue_move_up(&w.selected_ids()));
            }
        });
        action("down-queue", |w| {
            if w.imp().queues_enabled.get() && w.is_ready_for_torrent_action() {
                w.dispatch_interactive(p::torrent_queue_move_down(&w.selected_ids()));
            }
        });
        action("bottom-queue", |w| {
            if w.imp().queues_enabled.get() && w.is_ready_for_torrent_action() {
                w.dispatch_interactive(p::torrent_queue_move_bottom(&w.selected_ids()));
            }
        });
        action("top-queue", |w| {
            if w.imp().queues_enabled.get() && w.is_ready_for_torrent_action() {
                w.dispatch_interactive(p::torrent_queue_move_top(&w.selected_ids()));
            }
        });
        action("resume-all", |w| {
            if w.client().is_connected() {
                w.dispatch_interactive(p::torrent_start(None));
            }
        });
        action("pause-all", |w| {
            if w.client().is_connected() {
                w.dispatch_interactive(p::torrent_pause(None));
            }
        });
        action("local-prefs", |w| w.open_local_prefs());
        action("remote-prefs", |w| w.open_remote_prefs());
        action("view-stats", |w| {
            if w.client().is_connected() {
                dialogs::stats::instance(w, &w.client()).present();
            }
        });
        action("about", |w| {
            let about = dialogs::about::about_window_new(Some(w.upcast_ref()));
            about.set_modal(true);
            about.set_hide_on_close(false);
            about.present();
        });
    }

    fn action_notify(&self, name: &str, f: fn(&MainWindow, bool, bool)) {
        let Some(action) = self.lookup_action(name) else { return };
        let weak = self.downgrade();
        action.connect_notify_local(Some("state"), move |a, _| {
            let Some(win) = weak.upgrade() else { return };
            let a = a.downcast_ref::<gio::Action>().unwrap();
            let state = a.state().and_then(|s| s.get::<bool>()).unwrap_or(false);
            f(&win, a.is_enabled(), state);
        });
    }

    fn menu_bar_new(&self) -> Rc<MenuBar> {
        // Everything the menus and toolbar can do is a "win." action.
        self.add_actions();

        // This also adds the stateful view actions (check/radio items).
        let mb = Rc::new(MenuBar::new(self, self.client().prefs(), &self.torrent_tv()));

        self.action_notify("view-notebook", |w, _, active| w.imp().notebook.set_visible(active));
        self.action_notify("filter-dirs", |w, enabled, active| {
            if enabled {
                w.state_selector().set_show_dirs(active);
            }
        });
        self.action_notify("filter-trackers", |w, enabled, active| {
            if enabled {
                w.state_selector().set_show_trackers(active);
            }
        });
        self.action_notify("directories-first", |w, enabled, active| {
            if enabled {
                w.state_selector().set_directories_first(active);
            }
        });
        self.action_notify("view-states", |w, _, active| w.state_selector().set_visible(active));

        mb
    }

    fn set_limit(
        &self,
        action: &gio::SimpleAction,
        value: i32,
        speed_key: &str,
        enabled_key: &str,
        ids: Option<&[i64]>,
    ) {
        action.set_state(&value.to_variant());
        let mut req = match ids {
            Some(ids) => p::torrent_set(ids),
            None => p::session_set(),
        };
        if value >= 0 {
            req.set(speed_key, value);
        }
        req.set(enabled_key, value >= 0);

        if ids.is_some() {
            self.dispatch_interactive(req);
        } else {
            self.dispatch_session_set(req);
        }
    }

    fn priority_menu_new(&self, actions: &gio::SimpleActionGroup, ids: &[i64]) -> gio::MenuItem {
        let selected = self
            .client()
            .torrent_data(self.selected_torrent_id())
            .map(|(t, _)| get_i64(&t, p::FIELD_BANDWIDTH_PRIORITY))
            .unwrap_or(p::TR_PRI_UNSET);

        let action = gio::SimpleAction::new_stateful(
            "priority",
            Some(glib::VariantTy::INT32),
            &(selected as i32).to_variant(),
        );
        let weak = self.downgrade();
        let ids = ids.to_vec();
        action.connect_change_state(move |action, value| {
            let (Some(win), Some(v)) = (weak.upgrade(), value.and_then(|v| v.get::<i32>())) else {
                return;
            };
            action.set_state(&v.to_variant());
            let mut req = p::torrent_set(&ids);
            req.set(p::FIELD_BANDWIDTH_PRIORITY, v);
            win.dispatch_interactive(req);
        });
        actions.add_action(&action);

        let menu = gio::Menu::new();
        for (label, value) in
            [("High", p::TR_PRI_HIGH), ("Normal", p::TR_PRI_NORMAL), ("Low", p::TR_PRI_LOW)]
        {
            let item = gio::MenuItem::new(Some(&tr(label)), None);
            item.set_action_and_target_value(
                Some("torrent.priority"),
                Some(&(value as i32).to_variant()),
            );
            menu.append_item(&item);
        }

        let top = gio::MenuItem::new_submenu(Some(&tr("Priority")), &menu);
        top.set_icon(&gio::ThemedIcon::new("network-workgroup"));
        top
    }

    fn limit_menu_new(
        &self,
        actions: &gio::SimpleActionGroup,
        name: &str,
        title: &str,
        enabled_key: &'static str,
        speed_key: &'static str,
        ids: Option<&[i64]>,
    ) -> gio::MenuItem {
        let client = self.client();
        let current: Option<Obj> = match ids {
            Some(_) => client.torrent_data(self.selected_torrent_id()).map(|(t, _)| (*t).clone()),
            None => client.session_clone(),
        };
        let limit = current
            .as_ref()
            .map(|c| if get_bool(c, enabled_key) { get_i64(c, speed_key) } else { -1 })
            .unwrap_or(-1);

        let action = gio::SimpleAction::new_stateful(
            name,
            Some(glib::VariantTy::INT32),
            &(limit as i32).to_variant(),
        );
        let weak = self.downgrade();
        let ids_owned = ids.map(<[i64]>::to_vec);
        action.connect_change_state(move |action, value| {
            if let (Some(win), Some(v)) = (weak.upgrade(), value.and_then(|v| v.get::<i32>())) {
                win.set_limit(action, v, speed_key, enabled_key, ids_owned.as_deref());
            }
        });
        actions.add_action(&action);

        let detailed = format!("torrent.{name}");
        let menu = gio::Menu::new();
        let section = gio::Menu::new();
        let item = gio::MenuItem::new(Some(&tr("No Limit")), None);
        item.set_action_and_target_value(Some(&detailed), Some(&(-1i32).to_variant()));
        section.append_item(&item);

        // bandwidth items: n = 5*2^(n-1)
        let presets: Vec<i64> = (0..16).map(|i| 5i64 << i).collect();
        if limit >= 0 && !presets.contains(&limit) {
            // GTK 3 showed this checked but insensitive. A disabled action
            // doesn't show its state in a GTK 4 menu, so this is a normal
            // (checked) radio item, choosing it sets the same limit again.
            let item = gio::MenuItem::new(Some(&util::format_speed_kbps(limit as f64)), None);
            item.set_action_and_target_value(Some(&detailed), Some(&(limit as i32).to_variant()));
            section.append_item(&item);
        }
        menu.append_section(None, &section);

        let section = gio::Menu::new();
        for v in presets {
            let item = gio::MenuItem::new(Some(&util::format_speed_kbps(v as f64)), None);
            item.set_action_and_target_value(Some(&detailed), Some(&(v as i32).to_variant()));
            section.append_item(&item);
        }
        menu.append_section(None, &section);

        let top = gio::MenuItem::new_submenu(Some(title), &menu);
        top.set_icon(&gio::ThemedIcon::new("network-workgroup"));
        top
    }

    fn torrent_tv_view_menu(&self, tv: &TreeView, x: f64, y: f64) {
        let imp = self.imp();
        let actions = gio::SimpleActionGroup::new();
        let menu = gio::Menu::new();
        let ids = torrent_view::selected_ids(tv);

        menu.append_section(None, &imp.torrent_actions_menu.get());
        if imp.queues_enabled.get() {
            menu.append_section(None, &imp.torrent_queue_menu.get());
        }

        let section = gio::Menu::new();
        section.append_item(&self.limit_menu_new(
            &actions,
            "down-limit",
            &tr("Down Limit"),
            p::FIELD_DOWNLOAD_LIMITED,
            p::FIELD_DOWNLOAD_LIMIT,
            Some(&ids),
        ));
        section.append_item(&self.limit_menu_new(
            &actions,
            "up-limit",
            &tr("Up Limit"),
            p::FIELD_UPLOAD_LIMITED,
            p::FIELD_UPLOAD_LIMIT,
            Some(&ids),
        ));
        section.append_item(&self.priority_menu_new(&actions, &ids));
        menu.append_section(None, &section);

        util::popover_menu_popup(tv, &menu, x, y, "torrent", Some(&actions));
    }

    fn torrent_tv_context_menu(&self, tv: &TreeView, x: f64, y: f64) -> bool {
        // from the keyboard
        if x < 0.0 {
            self.torrent_tv_view_menu(tv, x, y);
            return true;
        }
        // the column headers have their own menu
        if tv.point_in_header(x, y) {
            return false;
        }
        if tv.select_at(x, y) {
            self.torrent_tv_view_menu(tv, x, y);
            return true;
        }
        false
    }

    fn session_updated(&self) {
        let imp = self.imp();
        let Some(session) = self.client().session_clone() else { return };
        imp.status_bar.session_update(&session);

        let queues_enabled = session.contains_key(p::SGET_DOWNLOAD_QUEUE_ENABLED)
            && (get_bool(&session, p::SGET_DOWNLOAD_QUEUE_ENABLED)
                || get_bool(&session, p::SGET_SEED_QUEUE_ENABLED));

        if imp.queues_enabled.get() != queues_enabled {
            self.menu_bar().set_supports_queues(queues_enabled);
            self.state_selector().set_queues_enabled(queues_enabled);
        }
        imp.queues_enabled.set(queues_enabled);
    }

    fn construct(&self) {
        let imp = self.imp();
        let client = self.client();
        let prefs = client.prefs().clone();

        imp.queues_enabled.set(true);
        imp.selected_torrent_id.set(-1);

        gtk::Window::set_default_icon_name(env!("TRG_PACKAGE_NAME"));

        let model = TorrentModel::new();
        client.set_torrent_model(&model);

        let weak = self.downgrade();
        model.connect_torrent_completed(move |item| {
            if let Some(win) = weak.upgrade() {
                win.torrent_event_notification(
                    "trg-gtk-apply",
                    &tr("This torrent has completed."),
                    prefs::KEY_COMPLETE_NOTIFY,
                    item,
                );
            }
        });
        let weak = self.downgrade();
        model.connect_torrent_added(move |item| {
            if let Some(win) = weak.upgrade() {
                win.torrent_event_notification(
                    "list-add",
                    &tr("This torrent has been added."),
                    prefs::KEY_ADD_NOTIFY,
                    item,
                );
            }
        });

        // the torrents shown, filtered by the state selector and the entry
        let weak = self.downgrade();
        let filter = gtk::CustomFilter::new(move |obj| {
            let (Some(win), Some(item)) = (weak.upgrade(), obj.downcast_ref::<Item>()) else {
                return true;
            };
            win.visible_func(item)
        });
        let filtered = gtk::FilterListModel::new(Some(model.store()), Some(filter.clone()));

        let tv = torrent_view::torrent_tree_view_new(&client, &filtered);
        {
            let mut parts = imp.parts.borrow_mut();
            parts.torrent_model = Some(model.clone());
            parts.torrent_filter = Some(filter);
            parts.torrent_tv = Some(tv.clone());
        }

        let selection = tv.selection();
        let weak = self.downgrade();
        selection.connect_selection_changed(move |_, _, _| {
            if let Some(win) = weak.upgrade() {
                win.update_selected_torrent_notebook(
                    p::TORRENT_GET_MODE_FIRST,
                    win.first_selected_id(),
                );
            }
        });
        // Torrents were added, removed, filtered or sorted. When the selected
        // torrent is gone, so is the selection, without a selection-changed.
        let weak = self.downgrade();
        selection.connect_items_changed(move |_, _, _, _| {
            if let Some(win) = weak.upgrade() {
                let id = win.first_selected_id();
                if id != win.selected_torrent_id() {
                    win.update_selected_torrent_notebook(p::TORRENT_GET_MODE_FIRST, id);
                }
            }
        });
        let weak = self.downgrade();
        util::add_context_menu(&tv, move |w, x, y| {
            let (Some(win), Some(tv)) = (weak.upgrade(), w.downcast_ref::<TreeView>()) else {
                return false;
            };
            win.torrent_tv_context_menu(tv, x, y)
        });
        let weak = self.downgrade();
        tv.connect_row_activated(move |_, _| {
            if let Some(win) = weak.upgrade() {
                win.open_props();
            }
        });

        // The parts the menu bar's actions need
        let selector = StateSelector::new(&client, &model);
        imp.parts.borrow_mut().state_selector = Some(selector.clone());
        self.notebook_fill();

        // The menu is a GMenuModel, shown here with a GtkPopoverMenuBar, or in
        // the macOS menu bar (see app.rs).
        let mb = self.menu_bar_new();
        if util::is_macos() {
            imp.menu_bar.set_visible(false);
        } else {
            imp.menu_bar.set_menu_model(Some(&mb.model()));
        }
        imp.parts.borrow_mut().menu_bar = Some(mb);

        let toolbar = Toolbar::new(&prefs);
        toolbar.set_hexpand(true);
        imp.toolbar_box.prepend(&toolbar);
        imp.parts.borrow_mut().toolbar = Some(toolbar);

        imp.hpaned.set_start_child(Some(&selector));
        imp.hpaned.set_end_child(Some(&tv));

        let weak = self.downgrade();
        selector.connect_state_changed(move |_, _| {
            if let Some(win) = weak.upgrade() {
                win.refilter();
            }
        });
        // the restored filter
        self.refilter();

        // the lists are enabled once connected (conn_changed)
        {
            let parts = imp.parts.borrow();
            tv.set_sensitive(false);
            parts.peers_tv.as_ref().unwrap().set_sensitive(false);
            parts.files_view.as_ref().unwrap().tv.set_sensitive(false);
            parts.trackers_view.as_ref().unwrap().tv.set_sensitive(false);
        }

        // There's no system tray in GTK 4

        imp.status_bar.set_client(self, &client);
        let weak = self.downgrade();
        client.connect_session_updated(move |_| {
            if let Some(win) = weak.upgrade() {
                win.session_updated();
            }
        });

        let width = prefs.get_int(prefs::KEY_WINDOW_WIDTH, prefs::GLOBAL);
        let height = prefs.get_int(prefs::KEY_WINDOW_HEIGHT, prefs::GLOBAL);
        let pos = prefs.get_int(prefs::KEY_NOTEBOOK_PANED_POS, prefs::GLOBAL);

        if width > 0 && height > 0 {
            self.set_default_size(width as i32, height as i32);
        } else if pos < 1 {
            imp.vpaned.set_position(300);
        }
        if pos > 0 {
            imp.vpaned.set_position(pos as i32);
        }

        selector.set_visible(prefs.get_bool(prefs::KEY_SHOW_STATE_SELECTOR, prefs::GLOBAL));
        imp.notebook.set_visible(prefs.get_bool(prefs::KEY_SHOW_NOTEBOOK, prefs::GLOBAL));

        let pos = prefs.get_int(prefs::KEY_STATES_PANED_POS, prefs::GLOBAL);
        if pos > 0 {
            imp.hpaned.set_position(pos as i32);
        }

        // Drag and Drop
        let drop = gtk::DropTarget::new(
            gdk::FileList::static_type(),
            gdk::DragAction::COPY | gdk::DragAction::MOVE,
        );
        let weak = self.downgrade();
        drop.connect_drop(move |_, value, _, _| {
            let (Some(win), Ok(files)) = (weak.upgrade(), value.get::<gdk::FileList>()) else {
                return false;
            };
            let paths: Vec<String> = files
                .files()
                .iter()
                .filter_map(|f| f.path().map(|p| p.to_string_lossy().into_owned()))
                .collect();
            win.add_from_filenames(paths);
            true
        });
        self.add_controller(drop);
    }

    pub fn set_start_args(&self, args: Vec<String>) {
        *self.imp().args.borrow_mut() = Some(args);
    }

    /// Adds torrents (files, URLs or magnet links) opened while running:
    /// straight away if connected, otherwise once connected, like the
    /// command line arguments.
    pub fn open(&self, uris: Vec<String>) {
        if self.client().is_connected() {
            self.add_from_filenames(uris);
        } else {
            let mut args = self.imp().args.borrow_mut();
            match args.as_mut() {
                Some(a) => a.extend(uris),
                None => *args = Some(uris),
            }
        }
        self.present();
    }

    pub fn auto_connect_if_required(&self) {
        let prefs = self.client().prefs().clone();
        let host = prefs.get_string(prefs::KEY_HOSTNAME, prefs::PROFILE).unwrap_or_default();
        if !host.is_empty() && prefs.get_bool(prefs::KEY_AUTO_CONNECT, prefs::PROFILE) {
            self.connect_profile(-1);
        }
    }
}

impl MainWindow {
    pub fn notebook_set_visible(&self, visible: bool) {
        self.imp().notebook.set_visible(visible);
    }
}
