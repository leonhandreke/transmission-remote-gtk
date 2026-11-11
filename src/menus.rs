//! The toolbar (trg-toolbar.c) and the main menu (trg-menu-bar.c).
//!
//! The main menu is a GMenuModel. The menu items activate actions installed
//! on the main window ("win." prefix), apart from the stateful view actions
//! (check and radio items) which are created here as they're backed by
//! preferences. It's shown in the window with a GtkPopoverMenuBar, or on
//! macOS in the system menu bar with gtk_application_set_menubar(). There
//! GTK provides the application menu, with About, Preferences and Quit
//! (app.about, app.preferences and app.quit), so those aren't repeated in the
//! other menus, and the Help menu links to the project page (app.help).

use std::cell::{Cell, RefCell};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, gio, glib};

use crate::i18n::tr;
use crate::main_window::MainWindow;
use crate::prefs::{self, Prefs};
use crate::tree_view::TreeView;
use crate::util;

/* The toolbar: a box of buttons with the "toolbar" style class. The buttons
 * activate the main window's actions, so they are made (in)sensitive along
 * with the menu items. */

mod tb_imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-toolbar.ui")]
    pub struct Toolbar {
        #[template_child]
        pub connect_menu_button: TemplateChild<gtk::MenuButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Toolbar {
        const NAME: &'static str = "TrgToolbar";
        type Type = super::Toolbar;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Toolbar {}
    impl WidgetImpl for Toolbar {}
    impl BoxImpl for Toolbar {}
}

glib::wrapper! {
    pub struct Toolbar(ObjectSubclass<tb_imp::Toolbar>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Toolbar {
    pub fn new(prefs: &Prefs) -> Self {
        let tb: Self = glib::Object::new();
        // The toolbar has a few buttons only, as in GNOME's and Apple's
        // guidelines (Properties and the preferences are in the menus), so
        // Disconnect goes in its own section of the profiles drop down. Like
        // the menu item, it's insensitive along with its action.
        let menu = gio::Menu::new();
        menu.append_section(None, &file_connect_menu_new(prefs));
        let disconnect = gio::Menu::new();
        disconnect.append(Some(&tr("Disconnect")), Some("win.disconnect"));
        menu.append_section(None, &disconnect);
        tb.imp().connect_menu_button.set_menu_model(Some(&menu));
        tb
    }
}

/* The main menu */

const CONNECTED_ACTIONS: &[&str] =
    &["add", "add-url", "disconnect", "remote-prefs", "view-stats", "resume-all", "pause-all"];

const TORRENT_ACTIONS: &[&str] = &[
    "properties",
    "copy-magnet",
    "remove",
    "delete",
    "resume",
    "pause",
    "verify",
    "reannounce",
    "move",
    "start-now",
    "up-queue",
    "down-queue",
    "top-queue",
    "bottom-queue",
];

const VIEW_DEPENDENT_ACTIONS: &[&str] = &["filter-dirs", "filter-trackers", "directories-first"];

/// "win.style" has the nick of the style as its state, so it can be the
/// target of the items in the GtkBuilder file
const STYLE_NICKS: &[&str] = &["transmission", "classic", "transmission-compact"];

fn set_enabled(win: &MainWindow, names: &[&str], enabled: bool) {
    for name in names {
        if let Some(a) = win.lookup_action(name).and_downcast::<gio::SimpleAction>() {
            a.set_enabled(enabled);
        }
    }
}

/// Appends (copies of) the items of a menu from the GtkBuilder file
fn append_items(menu: &gio::Menu, items: &gio::MenuModel) {
    for i in 0..items.n_items() {
        menu.append_item(&gio::MenuItem::from_model(items, i));
    }
}

fn style_state(prefs: &Prefs) -> glib::Variant {
    let mut style = prefs.get_int(prefs::KEY_STYLE, prefs::GLOBAL);
    if style < 0 || style as usize >= STYLE_NICKS.len() {
        style = prefs::STYLE_TR;
    }
    STYLE_NICKS[style as usize].to_variant()
}

fn file_connect_menu_fill(menu: &gio::Menu, prefs: &Prefs) {
    for (i, profile) in prefs.profiles().iter().enumerate() {
        let name = profile
            .get(prefs::KEY_PROFILE_NAME)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| tr(prefs::PROFILE_NAME_DEFAULT));
        // The connect action's state is the current profile, so the items
        // display as radio items like they did in GTK 3.
        let item = gio::MenuItem::new(Some(&name), None);
        item.set_action_and_target_value(Some("win.connect"), Some(&(i as i32).to_variant()));
        menu.append_item(&item);
    }
}

/// A menu of the connection profiles, which keeps itself up to date
pub fn file_connect_menu_new(prefs: &Prefs) -> gio::Menu {
    let menu = gio::Menu::new();
    file_connect_menu_fill(&menu, prefs);
    let weak = menu.downgrade();
    prefs.connect_profile_changed(move |prefs| {
        if let Some(menu) = weak.upgrade() {
            menu.remove_all();
            file_connect_menu_fill(&menu, prefs);
        }
    });
    menu
}

fn current_profile_index(prefs: &Prefs) -> i32 {
    prefs.profile().map(|p| p as i32).unwrap_or(-1)
}

pub struct MenuBar {
    pub menu: gio::Menu,
    queue_section: gio::Menu,
    queue_items: gio::MenuModel,
    queues_shown: Cell<bool>,
    win: glib::WeakRef<MainWindow>,
    _prefs: RefCell<Option<Prefs>>,
}

impl MenuBar {
    pub fn new(win: &MainWindow, prefs: &Prefs, torrent_tv: &TreeView) -> Self {
        let macos = util::is_macos();

        // "win.connect" connects to the profile index given (-1 for the
        // current one) and its state is the current profile.
        if let Some(connect) = win.lookup_action("connect").and_downcast::<gio::SimpleAction>() {
            connect.set_state(&current_profile_index(prefs).to_variant());
        }

        let builder =
            gtk::Builder::from_resource("/io/github/TransmissionRemoteGtk/ui/trg-menu-bar.ui");
        let menu: gio::Menu = builder.object("main_menu").unwrap();

        // The connect submenu of the File menu
        let connect_menu: gio::Menu = builder.object("connect_menu").unwrap();
        file_connect_menu_fill(&connect_menu, prefs);
        let weak = connect_menu.downgrade();
        prefs.connect_profile_changed(move |prefs| {
            if let Some(menu) = weak.upgrade() {
                menu.remove_all();
                file_connect_menu_fill(&menu, prefs);
            }
        });

        // Quit, or on macOS (where it's in the application menu) Close Window
        let file_menu: gio::Menu = builder.object("file_menu").unwrap();
        file_menu.remove(file_menu.n_items() - if macos { 2 } else { 1 });

        let queue_section: gio::Menu = builder.object("queue_section").unwrap();
        let queue_items: gio::MenuModel = builder.object("queue_items").unwrap();
        append_items(&queue_section, &queue_items);

        // macOS has Preferences in the application menu
        if macos {
            builder.object::<gio::Menu>("options_menu").unwrap().remove(0);
        }

        let mb = Self {
            menu: menu.clone(),
            queue_section,
            queue_items,
            queues_shown: Cell::new(true),
            win: win.downgrade(),
            _prefs: RefCell::new(Some(prefs.clone())),
        };

        mb.view_menu_fill(
            win,
            prefs,
            torrent_tv,
            &builder.object::<gio::Menu>("view_section").unwrap(),
        );

        if macos {
            let edit: gio::MenuModel = builder.object("macos_edit_menu").unwrap();
            menu.insert_item(1, &gio::MenuItem::from_model(&edit, 0));
            append_items(&menu, &builder.object::<gio::MenuModel>("macos_window_menu").unwrap());
            append_items(&menu, &builder.object::<gio::MenuModel>("macos_help_menu").unwrap());
        } else {
            append_items(&menu, &builder.object::<gio::MenuModel>("help_menu").unwrap());
        }

        mb.connected_change(false);
        mb.torrent_actions_sensitive(false);

        let w = win.downgrade();
        prefs.connect_profile_changed(move |prefs| {
            if let Some(connect) = w
                .upgrade()
                .and_then(|w| w.lookup_action("connect"))
                .and_downcast::<gio::SimpleAction>()
            {
                connect.set_state(&current_profile_index(prefs).to_variant());
            }
        });

        mb
    }

    pub fn model(&self) -> gio::MenuModel {
        self.menu.clone().upcast()
    }

    /// Menu models have no hidden items, so remove/re-add the queue items.
    /// An empty section isn't shown, along with its separator.
    pub fn set_supports_queues(&self, supports: bool) {
        if supports == self.queues_shown.get() {
            return;
        }
        if supports {
            append_items(&self.queue_section, &self.queue_items);
        } else {
            self.queue_section.remove_all();
        }
        self.queues_shown.set(supports);
    }

    pub fn connected_change(&self, connected: bool) {
        if let Some(win) = self.win.upgrade() {
            set_enabled(&win, CONNECTED_ACTIONS, connected);
        }
    }

    pub fn torrent_actions_sensitive(&self, sensitive: bool) {
        if let Some(win) = self.win.upgrade() {
            set_enabled(&win, TORRENT_ACTIONS, sensitive);
        }
    }

    fn style_action_new(win: &MainWindow, prefs: &Prefs) {
        let action = gio::SimpleAction::new_stateful(
            "style",
            Some(glib::VariantTy::STRING),
            &style_state(prefs),
        );
        let p = prefs.clone();
        action.connect_change_state(move |action, value| {
            let Some(nick) = value.and_then(|v| v.str().map(str::to_string)) else { return };
            if let Some(i) = STYLE_NICKS.iter().position(|n| *n == nick) {
                action.set_state(value.unwrap());
                p.set_int(prefs::KEY_STYLE, i as i64, prefs::GLOBAL);
            }
        });
        let weak = action.downgrade();
        prefs.connect_changed(move |prefs, key| {
            let Some(action) = weak.upgrade() else { return };
            let value = style_state(prefs);
            if key == Some(prefs::KEY_STYLE) && action.state().as_ref() != Some(&value) {
                action.set_state(&value);
            }
        });
        win.add_action(&action);
    }

    /// A boolean preference, as stateful "win.<name>" action
    fn view_item_new(
        win: &MainWindow,
        prefs: &Prefs,
        name: &str,
        key: &'static str,
        dependency: Option<&str>,
    ) {
        let action = gio::SimpleAction::new_stateful(
            name,
            None,
            &prefs.get_bool(key, prefs::GLOBAL).to_variant(),
        );
        win.add_action(&action);

        if let Some(dep) = dependency.and_then(|d| win.lookup_action(d)) {
            action.set_enabled(dep.state().and_then(|s| s.get::<bool>()).unwrap_or(false));
        }

        let p = prefs.clone();
        action.connect_change_state(move |action, value| {
            let Some(value) = value else { return };
            action.set_state(value);
            p.set_bool(key, value.get::<bool>().unwrap_or(false), prefs::GLOBAL);
        });

        // Preferences can also be changed from the local preferences dialog
        let weak = action.downgrade();
        prefs.connect_changed(move |prefs, updated| {
            let Some(action) = weak.upgrade() else { return };
            if updated != Some(key) {
                return;
            }
            let value = prefs.get_bool(key, prefs::GLOBAL);
            if action.state().and_then(|s| s.get::<bool>()) != Some(value) {
                action.set_state(&value.to_variant());
            }
        });
    }

    fn view_menu_fill(
        &self,
        win: &MainWindow,
        prefs: &Prefs,
        torrent_tv: &TreeView,
        view_section: &gio::Menu,
    ) {
        Self::style_action_new(win, prefs);

        let sort_menu = torrent_tv.sort_menu(win);
        view_section.insert_submenu(0, Some(&tr("Sort")), &sort_menu);

        Self::view_item_new(win, prefs, "view-states", prefs::KEY_SHOW_STATE_SELECTOR, None);
        Self::view_item_new(win, prefs, "filter-dirs", prefs::KEY_FILTER_DIRS, Some("view-states"));
        Self::view_item_new(
            win,
            prefs,
            "filter-trackers",
            prefs::KEY_FILTER_TRACKERS,
            Some("view-states"),
        );
        Self::view_item_new(
            win,
            prefs,
            "directories-first",
            prefs::KEY_DIRECTORIES_FIRST,
            Some("view-states"),
        );

        if let Some(view_states) = win.lookup_action("view-states") {
            let w = win.downgrade();
            view_states.connect_notify_local(Some("state"), move |action, _| {
                if let Some(win) = w.upgrade() {
                    let state = action
                        .downcast_ref::<gio::Action>()
                        .and_then(|a| a.state())
                        .and_then(|s| s.get::<bool>());
                    set_enabled(&win, VIEW_DEPENDENT_ACTIONS, state.unwrap_or(false));
                }
            });
        }

        Self::view_item_new(win, prefs, "view-notebook", prefs::KEY_SHOW_NOTEBOOK, None);
    }
}

/// On macOS, <Primary> means Command, which GTK itself doesn't do (it's
/// <Meta> there).
fn platform_accels(accels: &[&str], macos: bool) -> Vec<String> {
    accels
        .iter()
        .map(|a| if macos { a.replace("<Primary>", "<Meta>") } else { a.to_string() })
        .collect()
}

/// Keyboard shortcuts for the menu actions. These are application wide,
/// which is also what makes the menus display them.
pub fn set_accels(app: &gtk::Application) {
    let accels: &[(&str, &[&str])] = &[
        ("win.disconnect", &["<Primary>d"]),
        ("win.add", &["<Primary>o"]),
        ("win.add-url", &["<Primary>u"]),
        ("app.quit", &["<Primary>q"]),
        ("win.properties", &["<Primary>i"]),
        ("win.copy-magnet", &["<Primary>c"]),
        ("win.resume", &["<Primary>r"]),
        ("win.pause", &["<Primary>p"]),
        ("win.verify", &["<Primary>h"]),
        // Re-announce also had Ctrl+Q in GTK 3, clashing with Quit
        ("win.move", &["<Primary>m"]),
        ("win.remove", &["Delete"]),
        ("win.delete", &["<Shift>Delete"]),
        ("win.resume-all", &["<Shift><Primary>r"]),
        ("win.pause-all", &["<Shift><Primary>p"]),
        // Ctrl+comma is GNOME's Preferences shortcut; Ctrl+S would be Save
        ("win.local-prefs", &["<Primary>comma"]),
        ("win.remote-prefs", &["<Alt>s"]),
        // F9 is GNOME's sidebar shortcut (F2 would be Rename)
        ("win.view-states", &["F9"]),
        ("win.filter-dirs", &["F3"]),
        ("win.filter-trackers", &["F4"]),
        ("win.view-notebook", &["F5"]),
        ("win.view-stats", &["F7"]),
    ];
    // macOS conventions, and moving shortcuts that clash with the standard
    // ones: Command-C is Copy, Command-H Hide, Command-M Minimize and
    // Preferences is Command-comma (app.preferences, set by GTK).
    let macos_accels: &[(&str, &[&str])] = &[
        ("win.copy-magnet", &["<Shift><Primary>c"]),
        ("win.verify", &["<Shift><Primary>v"]),
        ("win.move", &["<Shift><Primary>m"]),
        ("win.remove", &["<Primary>BackSpace", "Delete"]),
        ("win.delete", &["<Alt><Primary>BackSpace", "<Shift>Delete"]),
        ("win.local-prefs", &[]),
        ("app.close-window", &["<Primary>w"]),
        ("app.minimize", &["<Primary>m"]),
        // Show/Hide Sidebar in Finder, Mail and Notes
        ("win.view-states", &["<Control><Primary>s"]),
    ];
    let macos = util::is_macos();

    for (action, a) in accels {
        let a = platform_accels(a, macos);
        app.set_accels_for_action(action, &a.iter().map(String::as_str).collect::<Vec<_>>());
    }
    if macos {
        for (action, a) in macos_accels {
            let a = platform_accels(a, true);
            app.set_accels_for_action(action, &a.iter().map(String::as_str).collect::<Vec<_>>());
        }
    }
}
