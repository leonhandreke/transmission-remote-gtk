//! The main window, which glues everything together: it connects to the server,
//! updates the torrents and the session periodically, and has the actions of
//! the menus and the toolbar.

use std::cell::{Cell, OnceCell, RefCell};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, gdk, gio, glib};

use crate::add_torrents::{self, AddOptions};
use crate::client::Client;
use crate::client::ConnectionState;
use crate::details::DetailLists;
use crate::dialogs::{
    self, preferences::PreferencesDialog, remote_prefs::RemotePrefsDialog, stats::StatsDialog,
};
use crate::general_panel::GeneralPanel;
use crate::i18n::{sprintf, tr};
use crate::item::Item;
use crate::json::get_bool;
use crate::menu_bar::MenuBar;
use crate::prefs::{self, Scope};
use crate::protocol as p;
use crate::state_selector::{FILTER_FLAG_DIR, FILTER_FLAG_TRACKER, StateSelector};
use crate::status_bar::StatusBar;
use crate::toolbar::Toolbar;
use crate::torrent_menu;
use crate::torrent_model::{self as tm, TorrentItemExt, TorrentModel, UpdateMode};
use crate::torrent_view;
use crate::tree_view::{PERSIST_LAYOUT, PERSIST_SORT, TreeView};
use crate::util;

/// The parts of the window that aren't in its template, made by construct()
pub struct Parts {
    torrent_tv: TreeView,
    torrent_filter: gtk::CustomFilter,
    toolbar: Toolbar,
    menu_bar: MenuBar,
    state_selector: StateSelector,
    details: DetailLists,
}

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/main-window.ui")]
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
        pub filter_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub torrent_overlay: TemplateChild<gtk::Overlay>,
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
        pub gen_details: TemplateChild<GeneralPanel>,
        #[template_child]
        pub status_bar: TemplateChild<StatusBar>,

        pub client: OnceCell<Client>,
        pub parts: OnceCell<Parts>,
        pub selected_torrent_id: Cell<i64>,

        // what the torrent filter shows, from the state selector and the entry
        pub filter_criteria: Cell<u32>,
        pub filter_selected_text: RefCell<Option<String>>,
        pub filter_text: RefCell<Option<String>>,

        pub queues_enabled: Cell<bool>,
        pub state_saved: Cell<bool>,
        // the sidebar's default width (0 until it's first shown), not saved
        pub states_default_pos: Cell<i32>,
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
    util::ancestor::<MainWindow>(w)
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
    fn entry_filter_changed_cb(entry: &gtk::SearchEntry) {
        if let Some(win) = main_window_of(entry.upcast_ref()) {
            win.refilter();
        }
    }

    /// Typing elsewhere in the window started a search: the next keys go to
    /// the entry, without replacing what was typed.
    #[template_callback]
    fn filter_search_started_cb(entry: &gtk::SearchEntry) {
        if let Some(text) = entry.delegate().and_downcast::<gtk::Text>() {
            text.grab_focus_without_selecting();
        }
    }

    /// Escape clears the filter, and then goes back to the list.
    #[template_callback]
    fn filter_stop_search_cb(entry: &gtk::SearchEntry) {
        if !entry.text().is_empty() {
            entry.set_text("");
        } else if let Some(win) = main_window_of(entry.upcast_ref()) {
            win.torrent_tv().grab_focus();
        }
    }
}

impl MainWindow {
    pub fn new(app: &gtk::Application, client: &Client) -> Self {
        let win: Self = glib::Object::builder().property("application", app).build();
        win.imp().client.set(client.clone()).expect("a new window has no client");
        win.construct();
        win
    }

    pub fn client(&self) -> Client {
        self.imp().client.get().expect("the client is set by new()").clone()
    }

    fn parts(&self) -> &Parts {
        self.imp().parts.get().expect("the parts are made by construct()")
    }

    fn torrent_tv(&self) -> TreeView {
        self.parts().torrent_tv.clone()
    }

    /// The window's own keys, before the focus widget and the app's
    /// shortcuts get them.
    fn key_pressed(
        &self,
        controller: &gtk::EventControllerKey,
        keyval: gdk::Key,
        state: gdk::ModifierType,
    ) -> bool {
        // Ctrl+K/Ctrl+F (Command-K/F on macOS)
        if state.contains(util::primary_modifier())
            && (keyval == gdk::Key::k || keyval == gdk::Key::f)
        {
            self.imp().filter_entry.grab_focus();
            return true;
        }

        // In a text field, the keys that are also shortcuts of the app (such
        // as Delete for Remove, or Ctrl+C) edit the text. Delete and
        // BackSpace never remove torrents from there.
        let Some(text) = RootExt::focus(self).and_downcast::<gtk::Text>() else {
            return false;
        };
        let accel = gtk::accelerator_name(keyval, state & gtk::accelerator_get_default_mod_mask());
        let is_shortcut =
            self.application().is_some_and(|app| !app.actions_for_accel(&accel).is_empty());
        if !is_shortcut {
            return false;
        }
        controller.forward(&text)
            || matches!(keyval, gdk::Key::Delete | gdk::Key::KP_Delete | gdk::Key::BackSpace)
    }

    /// "No Results" over the torrent list, when there's filter text and no
    /// torrent matches it
    fn update_no_results(&self) {
        let imp = self.imp();
        let Some(tv) = imp.parts.get().map(|p| &p.torrent_tv) else { return };
        let none = !imp.filter_entry.text().is_empty()
            && tv.selection().n_items() == 0
            && self.client().is_connected();
        imp.no_results.set_visible(none);
    }

    /// Shows the connection in the menus, the server picker, and "Not
    /// Connected" over the empty torrent list. Its button connects to the
    /// current profile, or opens the profiles if it has no host yet.
    fn update_connection(&self) {
        let imp = self.imp();
        let client = self.client();
        let prefs = client.prefs();
        let state = client.connection_state();
        let profile = match state {
            ConnectionState::Disconnected => Scope::Profile,
            _ => Scope::Connection,
        };
        let name = prefs.get_string(prefs::KEY_PROFILE_NAME, profile).unwrap_or_default();
        self.menu_bar().connection_change(state);
        self.toolbar().set_connection(state, &name);

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
        imp.connecting_label.set_label(&sprintf(&tr("Connecting to %s…"), &[&name]));
        imp.not_connected.set_visible(state != ConnectionState::Connected);
    }

    fn torrent_model(&self) -> TorrentModel {
        self.client().torrent_model()
    }

    fn toolbar(&self) -> &Toolbar {
        &self.parts().toolbar
    }

    fn menu_bar(&self) -> &MenuBar {
        &self.parts().menu_bar
    }

    pub fn state_selector(&self) -> StateSelector {
        self.parts().state_selector.clone()
    }

    pub fn selected_torrent_id(&self) -> i64 {
        self.imp().selected_torrent_id.get()
    }

    pub fn menu_model(&self) -> gio::MenuModel {
        self.menu_bar().model()
    }

    fn update_selected_torrent_notebook(&self, mode: UpdateMode, id: i64) {
        let client = self.client();
        let serial = client.serial();
        let data = if id >= 0 { client.torrent_data(id) } else { None };

        match data {
            Some((t, item)) => {
                self.menu_bar().torrent_actions_sensitive(true);
                self.imp().gen_details.update(&t, &item);
                self.parts().details.update(&t, serial, mode);
            }
            None => self.torrent_scrub(),
        }

        self.imp().selected_torrent_id.set(id);
    }

    fn torrent_event_notification(&self, icon_name: &str, desc: &str, pref_key: &str, item: &Item) {
        if !self.client().prefs().get_bool(pref_key, Scope::Global) {
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
        prefs.set_int(prefs::KEY_WINDOW_HEIGHT, height as i64, Scope::Global);
        prefs.set_int(prefs::KEY_WINDOW_WIDTH, width as i64, Scope::Global);
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
        prefs.set_int(prefs::KEY_NOTEBOOK_PANED_POS, imp.vpaned.position() as i64, Scope::Global);
        // Only a sidebar width that was chosen (by dragging), otherwise it
        // follows the width of its states. The default is a set position too.
        let pos = imp.hpaned.position();
        let dragged = imp.hpaned.is_position_set() && pos != imp.states_default_pos.get();
        let states_pos = if dragged { pos } else { 0 };
        prefs.set_int(prefs::KEY_STATES_PANED_POS, states_pos as i64, Scope::Global);

        let parts = self.parts();
        parts.details.persist();
        parts.torrent_tv.persist(PERSIST_SORT | PERSIST_LAYOUT);
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
            self.clipboard().set_text(&t.magnet_link);
        }
    }

    fn selected_ids(&self) -> Vec<i64> {
        torrent_view::selected_ids(&self.torrent_tv())
    }

    /// Adds torrents (files, URLs or magnet links), with the add dialog if
    /// that's what the preferences say
    pub fn add_from_filenames(&self, uris: Vec<String>) {
        let client = self.client();
        if !client.is_connected() || uris.is_empty() {
            return;
        }
        let prefs = client.prefs();
        if prefs.get_bool(prefs::KEY_ADD_OPTIONS_DIALOG, Scope::Global) {
            dialogs::add::TorrentAddDialog::new_from_filenames(self, &client, uris).present();
        } else {
            add_torrents::add_torrents(self, &client, &uris, &AddOptions::from_prefs(prefs));
        }
    }

    /// Connects to the profile with index, or the current one for -1
    pub fn connect_profile(&self, index: i32) {
        if let Err(msg) = self.client().connect_to_profile(index) {
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
        }
    }

    fn preferences_dialog(&self) -> PreferencesDialog {
        dialogs::single(|| PreferencesDialog::new(self, &self.client()))
    }

    fn open_local_prefs(&self) {
        self.preferences_dialog().present();
    }

    /// The local preferences on the Connection tab, which has the profiles
    fn edit_profiles(&self) {
        let prefs = self.preferences_dialog();
        prefs.set_page(0);
        prefs.present();
    }

    fn open_remote_prefs(&self) {
        if self.client().is_connected() {
            dialogs::single(|| RemotePrefsDialog::new(self, &self.client())).present();
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
            let client = win.client();
            let c = client.clone();
            client.dispatch_interactive_with(p::torrent_remove(&ids, delete_data), move |r| {
                // the free space changed
                if delete_data && r.is_ok() {
                    c.refresh_session();
                }
            });
        });
    }

    /// The client updated the torrents: the filter, the sorting, the details
    /// and the status bar follow
    fn torrents_updated(&self, mode: UpdateMode) {
        let imp = self.imp();
        let client = self.client();

        // The filter and sorter don't watch the items, so the values they
        // look at may have changed.
        if imp.filter_criteria.get() != 0 || imp.filter_text.borrow().is_some() {
            self.parts().torrent_filter.changed(gtk::FilterChange::Different);
        }
        self.torrent_tv().resort();

        self.update_selected_torrent_notebook(mode, self.selected_torrent_id());
        imp.status_bar.update(&self.torrent_model().stats(), &client);

        // files to add once connected
        if mode == UpdateMode::First
            && let Some(args) = imp.args.take()
        {
            self.add_from_filenames(args);
        }
    }

    fn visible_func(&self, item: &Item) -> bool {
        let imp = self.imp();
        let criteria = imp.filter_criteria.get();

        if criteria != 0 {
            if criteria & FILTER_FLAG_TRACKER != 0 {
                let Some(t) = item.torrent() else { return false };
                if !t.has_tracker(imp.filter_selected_text.borrow().as_deref()) {
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
        let Some(parts) = imp.parts.get() else { return };
        let selector = &parts.state_selector;
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

        parts.torrent_filter.changed(gtk::FilterChange::Different);
        self.update_no_results();
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
        self.parts().details.clear();
        imp.gen_details.clear();
        self.menu_bar().torrent_actions_sensitive(false);
    }

    /// The client connected, disconnected, or started connecting
    fn connection_changed(&self) {
        let imp = self.imp();
        let client = self.client();
        let state = client.connection_state();

        let connected = state == ConnectionState::Connected;
        let parts = self.parts();
        parts.torrent_tv.set_sensitive(connected);
        parts.details.set_sensitive(connected);
        imp.gen_details.set_sensitive(connected);

        let status_bar = &imp.status_bar;
        match state {
            ConnectionState::Connected => {
                if let Some(session) = client.session_clone() {
                    status_bar.connect(&session, &client);
                }
            }
            ConnectionState::Connecting => {
                status_bar.reset();
                status_bar.push_connection_msg(&tr("Connecting..."));
            }
            ConnectionState::Disconnected => {
                // not added when connecting failed
                imp.args.take();
                self.torrent_scrub();
                self.state_selector().disconnect();
                status_bar.reset();
            }
        }

        self.update_no_results();
        self.update_connection();
    }

    /// A "win." action without a parameter
    fn add_window_action(&self, name: &str, f: impl Fn(&Self) + 'static) {
        let action = gio::SimpleAction::new(name, None);
        let weak = self.downgrade();
        action.connect_activate(move |_, _| {
            if let Some(win) = weak.upgrade() {
                f(&win);
            }
        });
        self.add_action(&action);
    }

    fn add_actions(&self) {
        let action = |name: &str, f: fn(&MainWindow)| self.add_window_action(name, f);

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

        action("disconnect", |w| w.client().disconnect());
        action("add", |w| {
            if w.client().is_connected() {
                dialogs::add::add_dialog(w, &w.client());
            }
        });
        action("add-url", |w| dialogs::add_url::TorrentAddUrlDialog::new(w, &w.client()).present());
        action("properties", |w| w.open_props());
        action("copy-magnet", |w| w.copy_magnet_link());
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
        action("connect-current", |w| w.connect_profile(-1));
        action("local-prefs", |w| w.open_local_prefs());
        action("edit-profiles", |w| w.edit_profiles());
        action("remote-prefs", |w| w.open_remote_prefs());
        action("view-stats", |w| {
            if w.client().is_connected() {
                dialogs::single(|| StatsDialog::new(w, &w.client())).present();
            }
        });
        action("about", |w| {
            let about = dialogs::about::about_window_new(Some(w.upcast_ref()));
            about.set_modal(true);
            about.set_hide_on_close(false);
            about.present();
        });

        // Requests about the selected torrents (or all of them), sent when
        // connected, and for some when a torrent is selected, or the queues
        // are enabled
        let connected: fn(&MainWindow) -> bool = |w| w.client().is_connected();
        let selected: fn(&MainWindow) -> bool = MainWindow::is_ready_for_torrent_action;
        let queued: fn(&MainWindow) -> bool =
            |w| w.imp().queues_enabled.get() && w.is_ready_for_torrent_action();
        let request =
            |name: &str, guard: fn(&MainWindow) -> bool, req: fn(&[i64]) -> p::Request| {
                self.add_window_action(name, move |w| {
                    if guard(w) {
                        w.client().dispatch_interactive(req(&w.selected_ids()));
                    }
                });
            };
        request("resume", connected, |ids| p::torrent_start(Some(ids)));
        request("pause", connected, |ids| p::torrent_pause(Some(ids)));
        request("verify", selected, p::torrent_verify);
        request("reannounce", connected, p::torrent_reannounce);
        request("start-now", selected, p::torrent_start_now);
        request("up-queue", queued, p::torrent_queue_move_up);
        request("down-queue", queued, p::torrent_queue_move_down);
        request("top-queue", queued, p::torrent_queue_move_top);
        request("bottom-queue", queued, p::torrent_queue_move_bottom);
        request("resume-all", connected, |_| p::torrent_start(None));
        request("pause-all", connected, |_| p::torrent_pause(None));
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

    /// The view actions (made by the menu bar) show and hide the parts
    fn connect_view_actions(&self) {
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
    }

    fn torrent_tv_context_menu(&self, tv: &TreeView, x: f64, y: f64) -> bool {
        if !tv.context_menu_for_rows(x, y) {
            return false;
        }
        let imp = self.imp();
        let mut sections = vec![imp.torrent_actions_menu.get()];
        if imp.queues_enabled.get() {
            sections.push(imp.torrent_queue_menu.get());
        }
        torrent_menu::popup(self, tv, x, y, &sections);
        true
    }

    fn session_updated(&self) {
        let imp = self.imp();
        let client = self.client();
        let Some(session) = client.session_clone() else { return };
        imp.status_bar.session_update(&session);
        // the speed limits may have changed
        imp.status_bar.update_speed(&self.torrent_model().stats(), &client);

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

        let model = client.torrent_model();

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
        let selector = StateSelector::new(&client, &model);
        let details = DetailLists::new(self, &client, "");
        for (tv, title) in details.pages() {
            imp.notebook.append_page(tv, Some(&gtk::Label::new(Some(&title))));
        }

        // Everything the menus and toolbar can do is a "win." action. The menu
        // bar adds the stateful view actions (check and radio items).
        self.add_actions();
        let menu_bar = MenuBar::new(self, &prefs, &tv);
        let toolbar = Toolbar::new(&prefs);

        let parts = Parts {
            torrent_tv: tv.clone(),
            torrent_filter: filter,
            toolbar: toolbar.clone(),
            menu_bar,
            state_selector: selector.clone(),
            details,
        };
        if imp.parts.set(parts).is_err() {
            unreachable!("a window is constructed once");
        }
        self.connect_view_actions();

        let selection = tv.selection();
        let weak = self.downgrade();
        selection.connect_selection_changed(move |_, _, _| {
            if let Some(win) = weak.upgrade() {
                win.update_selected_torrent_notebook(UpdateMode::First, win.first_selected_id());
            }
        });
        // Torrents were added, removed, filtered or sorted. When the selected
        // torrent is gone, so is the selection, without a selection-changed.
        let weak = self.downgrade();
        selection.connect_items_changed(move |_, _, _, _| {
            if let Some(win) = weak.upgrade() {
                let id = win.first_selected_id();
                if id != win.selected_torrent_id() {
                    win.update_selected_torrent_notebook(UpdateMode::First, id);
                }
                win.update_no_results();
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

        // The menu is a GMenuModel, shown here with a GtkPopoverMenuBar, or in
        // the macOS menu bar (see app.rs).
        if util::is_macos() {
            imp.menu_bar.set_visible(false);
        } else {
            imp.menu_bar.set_menu_model(Some(&self.menu_bar().model()));
        }

        toolbar.set_hexpand(true);
        imp.toolbar_box.prepend(&toolbar);

        imp.hpaned.set_start_child(Some(&selector));
        imp.torrent_overlay.set_child(Some(&tv));
        // Not wider than the list, when it's narrow
        imp.torrent_overlay.set_clip_overlay(&*imp.no_results, true);
        imp.torrent_overlay.set_clip_overlay(&*imp.not_connected, true);
        // the current profile (None) and its name and host
        let weak = self.downgrade();
        prefs.connect_changed(move |_, key| {
            if let (Some(win), None | Some(prefs::KEY_PROFILE_NAME | prefs::KEY_HOSTNAME)) =
                (weak.upgrade(), key)
            {
                win.update_connection();
            }
        });
        self.update_connection();

        // Typing anywhere in the window filters the torrents, unless the
        // focus widget takes the keys: a text field, or a list with its own
        // type ahead (the sidebar, the details' lists). So the torrent list
        // has none.
        imp.filter_entry.set_key_capture_widget(Some(self));
        // The app's shortcuts are handled in the window's capture phase by a
        // controller added with the application. This one is added after
        // it, so it comes first.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = self.downgrade();
        keys.connect_key_pressed(move |c, keyval, _, state| match weak.upgrade() {
            Some(win) if win.key_pressed(c, keyval, state) => glib::Propagation::Stop,
            _ => glib::Propagation::Proceed,
        });
        self.add_controller(keys);

        let weak = self.downgrade();
        selector.connect_state_changed(move |_, _| {
            if let Some(win) = weak.upgrade() {
                win.refilter();
            }
        });
        // the restored filter
        self.refilter();

        // the lists are enabled once connected (connection_changed)
        tv.set_sensitive(false);
        self.parts().details.set_sensitive(false);

        imp.status_bar.set_client(&client);
        let weak = self.downgrade();
        client.connect_session_updated(move |_| {
            if let Some(win) = weak.upgrade() {
                win.session_updated();
            }
        });
        let weak = self.downgrade();
        client.connect_connection_changed(move |_| {
            if let Some(win) = weak.upgrade() {
                win.connection_changed();
            }
        });
        let weak = self.downgrade();
        client.connect_torrents_updated(move |_, mode| {
            if let Some(win) = weak.upgrade() {
                win.torrents_updated(mode);
            }
        });
        let weak = self.downgrade();
        client.connect_request_failed(move |_, msg| {
            if let Some(win) = weak.upgrade() {
                win.show_error(msg);
            }
        });
        let weak = self.downgrade();
        client.connect_status_message(move |_, msg| {
            if let Some(win) = weak.upgrade() {
                win.imp().status_bar.push_connection_msg(msg);
            }
        });

        let width = prefs.get_int(prefs::KEY_WINDOW_WIDTH, Scope::Global);
        let height = prefs.get_int(prefs::KEY_WINDOW_HEIGHT, Scope::Global);
        let pos = prefs.get_int(prefs::KEY_NOTEBOOK_PANED_POS, Scope::Global);

        if width > 0 && height > 0 {
            self.set_default_size(width as i32, height as i32);
        } else if pos < 1 {
            imp.vpaned.set_position(300);
        }
        if pos > 0 {
            imp.vpaned.set_position(pos as i32);
        }

        selector.set_visible(prefs.get_bool(prefs::KEY_SHOW_STATE_SELECTOR, Scope::Global));
        imp.notebook.set_visible(prefs.get_bool(prefs::KEY_SHOW_NOTEBOOK, Scope::Global));

        // A saved position wider than a third of the window is ignored, like
        // those saved when long directory names widened the sidebar.
        let pos = prefs.get_int(prefs::KEY_STATES_PANED_POS, Scope::Global);
        let window_width = if width > 0 { width } else { self.default_width() as i64 };
        if pos > 0 && pos <= window_width / 3 {
            imp.hpaned.set_position(pos as i32);
        }
        // Without one, the sidebar is a fifth wider than its states (its
        // minimum width), for a bit of slack. That's measured once it's shown,
        // rather than a fixed width, so it follows the fonts, the theme and
        // the translations. It can still be dragged down to the minimum.
        let weak = self.downgrade();
        selector.connect_map(move |selector| {
            let Some(win) = weak.upgrade() else { return };
            let paned = &win.imp().hpaned;
            if !paned.is_position_set() {
                let (min, ..) = selector.measure(gtk::Orientation::Horizontal, -1);
                let pos = (f64::from(min) * 1.2).round() as i32;
                win.imp().states_default_pos.set(pos);
                paned.set_position(pos);
            }
        });

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
        let host = prefs.get_string(prefs::KEY_HOSTNAME, Scope::Profile).unwrap_or_default();
        if !host.is_empty() && prefs.get_bool(prefs::KEY_AUTO_CONNECT, Scope::Profile) {
            self.connect_profile(-1);
        }
    }
}

impl MainWindow {
    pub fn notebook_set_visible(&self, visible: bool) {
        self.imp().notebook.set_visible(visible);
    }
}
