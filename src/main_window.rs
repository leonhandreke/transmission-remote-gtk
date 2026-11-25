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
use crate::details::Details;
use crate::dialogs::{
    self, preferences::PreferencesDialog, remote_prefs::RemotePrefsDialog, stats::StatsDialog,
};
use crate::i18n::{sprintf, tr};
use crate::menu_bar::MenuBar;
use crate::prefs::{self, Prefs, Scope};
use crate::protocol as p;
use crate::state_selector::StateSelector;
use crate::status_bar::StatusBar;
use crate::toolbar::Toolbar;
use crate::torrent_list::TorrentList;
use crate::torrent_model::{TorrentModel, UpdateMode};
use crate::torrent_object::TorrentObject;
use crate::util;

/// The parts of the window that aren't in its template, made by construct()
pub struct Parts {
    toolbar: Toolbar,
    menu_bar: MenuBar,
    state_selector: StateSelector,
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
        pub torrent_list: TemplateChild<TorrentList>,
        #[template_child]
        pub details: TemplateChild<Details>,
        #[template_child]
        pub status_bar: TemplateChild<StatusBar>,

        pub client: OnceCell<Client>,
        pub parts: OnceCell<Parts>,
        pub queues_enabled: Cell<bool>,
        pub state_saved: Cell<bool>,
        // whether it has connected since the start
        pub connected_once: Cell<bool>,
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
            Details::ensure_type();
            StatusBar::ensure_type();
            TorrentList::ensure_type();
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

    fn torrent_list(&self) -> TorrentList {
        self.imp().torrent_list.get()
    }

    fn details(&self) -> Details {
        self.imp().details.get()
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
            self.torrent_list().focus_search();
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

    /// Shows the connection in the menus, the server picker, and "Not
    /// Connected" over the empty torrent list. Its button connects to the
    /// current profile, or opens the profiles if it has no host yet.
    fn update_connection(&self) {
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
        self.torrent_list().set_connection(state, &name);
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

    fn state_selector(&self) -> StateSelector {
        self.parts().state_selector.clone()
    }

    pub fn selected_torrent_id(&self) -> i64 {
        self.torrent_list().selected_id()
    }

    pub fn menu_model(&self) -> gio::MenuModel {
        self.menu_bar().model()
    }

    /// The first selected torrent, as of the torrent list's last
    /// selection-changed
    fn selected_torrent(&self) -> Option<TorrentObject> {
        let id = self.selected_torrent_id();
        if id >= 0 { self.client().find_torrent(id) } else { None }
    }

    /// Shows the selected torrent (selected, or updated as mode says) in the
    /// details. The torrent actions are enabled while there's one.
    fn show_selected_torrent(&self, mode: UpdateMode) {
        let torrent = self.selected_torrent();
        self.menu_bar().torrent_actions_sensitive(torrent.is_some());
        self.details().set_torrent(torrent.as_ref(), mode);
    }

    fn torrent_event_notification(
        &self,
        icon_name: &str,
        desc: &str,
        pref_key: &str,
        torrent: &TorrentObject,
    ) {
        if !self.client().prefs().get_bool(pref_key, Scope::Global) {
            return;
        }
        let name = torrent.values().name.clone();
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

        // Closing the preferences dialog without OK restores the view
        // settings it previews, so they aren't saved here
        let windows = gtk::Window::toplevels();
        let dialogs: Vec<PreferencesDialog> =
            (0..windows.n_items()).filter_map(|i| windows.item(i).and_downcast()).collect();
        for dialog in dialogs {
            dialog.close();
        }

        self.save_window_size();
        prefs.set_int(prefs::KEY_NOTEBOOK_PANED_POS, imp.vpaned.position() as i64, Scope::Global);
        // Only a sidebar width that was chosen (by dragging), otherwise it
        // follows the width of its states. The default is a set position too.
        let pos = imp.hpaned.position();
        let dragged = imp.hpaned.is_position_set() && pos != imp.states_default_pos.get();
        let states_pos = if dragged { pos } else { 0 };
        prefs.set_int(prefs::KEY_STATES_PANED_POS, states_pos as i64, Scope::Global);

        self.details().persist();
        self.torrent_list().persist();
        prefs.save();

        imp.state_saved.set(true);
    }

    fn open_props(&self) {
        if self.selected_torrent_id() < 0 {
            return;
        }
        dialogs::props::TorrentPropsDialog::new(self, self.selected_ids(), &self.client())
            .present();
    }

    fn copy_magnet_link(&self) {
        if let Some(obj) = self.selected_torrent() {
            self.clipboard().set_text(&obj.torrent().magnet_link);
        }
    }

    fn selected_ids(&self) -> Vec<i64> {
        self.torrent_list().selected_ids()
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
        if let Err(e) = self.client().connect_to_profile(index) {
            // open the preferences once the (modal) error is dismissed
            let weak = self.downgrade();
            util::error_dialog_full(
                Some(self),
                &e.to_string(),
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
        let count = ids.len();
        let markup = if count == 1 {
            let name = self
                .torrent_list()
                .first_selected()
                .map(|t| t.values().name.clone())
                .unwrap_or_default();
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

        self.torrent_list().refilter_and_resort();

        self.show_selected_torrent(mode);
        imp.status_bar.update(&self.torrent_model().stats(), &client);

        // files to add once connected
        if mode == UpdateMode::First
            && let Some(args) = imp.args.take()
        {
            self.add_from_filenames(args);
        }
    }

    fn show_error(&self, msg: &str) {
        let sb = &self.imp().status_bar;
        sb.clear_indicators();
        sb.push_connection_msg(msg);
        util::error_dialog(Some(self), msg);
    }

    /// The client connected, disconnected, or started connecting
    fn connection_changed(&self) {
        let imp = self.imp();
        let client = self.client();
        let state = client.connection_state();

        // sensitive while connected, cleared once disconnected
        self.details().set_connection(state);

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
                self.menu_bar().torrent_actions_sensitive(false);
                self.state_selector().disconnect();
                status_bar.reset();
            }
        }

        self.update_connection();
        if state == ConnectionState::Connected {
            self.focus_torrent_list();
        }
    }

    /// Once connected (the list is insensitive until then), the torrent list
    /// gets the focus, so the arrows move in it: at the start, GTK gave it
    /// to the first widget that could take it, the toolbar's server picker;
    /// later, unless another widget has it (the Connect button that had it
    /// is hidden now). Not while a text field or a menu has it.
    fn focus_torrent_list(&self) {
        let first = !self.imp().connected_once.replace(true);
        let focus = RootExt::focus(self).filter(|f| f.is_mapped() && f.is_sensitive());
        let keep = focus.is_some_and(|f| {
            !first || f.is::<gtk::Text>() || f.ancestor(gtk::Popover::static_type()).is_some()
        });
        if !keep {
            self.torrent_list().grab_focus();
        }
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
                dialogs::move_dialog::TorrentMoveDialog::new(w, &w.client(), w.selected_ids())
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

    /// Shows the sidebar and the details as the preferences say, and
    /// follows them: the menu's view items and the preferences dialog set
    /// them. (The sidebar follows its own options, the torrent view the
    /// style.)
    fn follow_view_prefs(&self, prefs: &Prefs) {
        let show = |win: &Self, prefs: &Prefs| {
            let shown = |key| prefs.get_bool(key, Scope::Global);
            win.state_selector().set_visible(shown(prefs::KEY_SHOW_STATE_SELECTOR));
            win.details().set_visible(shown(prefs::KEY_SHOW_NOTEBOOK));
        };
        show(self, prefs);
        let weak = self.downgrade();
        prefs.connect_changed(move |prefs, key| {
            if let (Some(win), Some(prefs::KEY_SHOW_STATE_SELECTOR | prefs::KEY_SHOW_NOTEBOOK)) =
                (weak.upgrade(), key)
            {
                show(&win, prefs);
            }
        });
    }

    fn session_updated(&self) {
        let imp = self.imp();
        let client = self.client();
        let Some(session) = client.session_clone() else { return };
        imp.status_bar.session_update(&session);
        // the speed limits may have changed
        imp.status_bar.update_speed(&self.torrent_model().stats(), &client);

        let queues_enabled = session.queues_enabled();

        if imp.queues_enabled.get() != queues_enabled {
            self.menu_bar().set_supports_queues(queues_enabled);
            self.state_selector().set_queues_enabled(queues_enabled);
            self.torrent_list().set_queues_enabled(queues_enabled);
        }
        imp.queues_enabled.set(queues_enabled);
    }

    fn construct(&self) {
        let imp = self.imp();
        let client = self.client();
        let prefs = client.prefs().clone();

        imp.queues_enabled.set(true);

        gtk::Window::set_default_icon_name(env!("TRG_PACKAGE_NAME"));

        let model = client.torrent_model();

        let weak = self.downgrade();
        model.connect_torrent_completed(move |torrent| {
            if let Some(win) = weak.upgrade() {
                win.torrent_event_notification(
                    "trg-gtk-apply",
                    &tr("This torrent has completed."),
                    prefs::KEY_COMPLETE_NOTIFY,
                    torrent,
                );
            }
        });
        let weak = self.downgrade();
        model.connect_torrent_added(move |torrent| {
            if let Some(win) = weak.upgrade() {
                win.torrent_event_notification(
                    "list-add",
                    &tr("This torrent has been added."),
                    prefs::KEY_ADD_NOTIFY,
                    torrent,
                );
            }
        });

        let list = self.torrent_list();
        list.set_client(&client);
        let tv = list.tree_view();
        let selector = StateSelector::new(&client, &model);
        self.details().set_client(&client);

        // Everything the menus and toolbar can do is a "win." action. The menu
        // bar adds the stateful view actions (check and radio items).
        self.add_actions();
        let menu_bar = MenuBar::new(self, &prefs, &tv);
        let toolbar = Toolbar::new(&prefs);

        let parts = Parts { toolbar: toolbar.clone(), menu_bar, state_selector: selector.clone() };
        if imp.parts.set(parts).is_err() {
            unreachable!("a window is constructed once");
        }

        let weak = self.downgrade();
        list.connect_selection_changed(move |_| {
            if let Some(win) = weak.upgrade() {
                win.show_selected_torrent(UpdateMode::First);
            }
        });
        let weak = self.downgrade();
        list.connect_row_activated(move |_| {
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
        imp.toolbar_box.append(&toolbar);
        let search = list.search_entry();
        imp.toolbar_box.append(&search);

        imp.hpaned.set_start_child(Some(&selector));
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
        search.set_key_capture_widget(Some(self));
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

        let weak = list.downgrade();
        selector.connect_filter_changed(move |selector| {
            if let Some(list) = weak.upgrade() {
                list.set_filter(selector.filter());
            }
        });
        // the restored filter
        list.set_filter(selector.filter());

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

        self.follow_view_prefs(&prefs);

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
