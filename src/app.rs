//! The application (trg-gtk-app.c): the main window, the command line,
//! files opened from the Finder, and the macOS conventions.

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

use crate::client::Client;
use crate::main_window::MainWindow;
use crate::menus;
use crate::util;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct App {
        pub client: RefCell<Option<Client>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for App {
        const NAME: &'static str = "TrgGtkApp";
        type Type = super::App;
        type ParentType = gtk::Application;
    }

    impl ObjectImpl for App {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_inactivity_timeout(10000);
        }
    }

    impl ApplicationImpl for App {
        fn startup(&self) {
            self.parent_startup();
            self.obj().startup_main();
        }

        fn command_line(&self, cmdline: &gio::ApplicationCommandLine) -> glib::ExitCode {
            self.obj().handle_command_line(cmdline)
        }

        /// Files opened from the Finder or dropped on the Dock icon on
        /// macOS, whether or not the application was already running.
        fn open(&self, files: &[gio::File], _hint: &str) {
            let Some(win) = self.obj().main_window() else { return };
            let paths = files
                .iter()
                .filter_map(|f| f.path().map(|p| p.to_string_lossy().into_owned()))
                .collect();
            win.open(paths);
        }
    }

    impl GtkApplicationImpl for App {}
}

glib::wrapper! {
    pub struct App(ObjectSubclass<imp::App>)
        @extends gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl App {
    pub fn new(client: &Client) -> Self {
        let mut flags = gio::ApplicationFlags::HANDLES_COMMAND_LINE;
        // The macOS backend passes on files opened from the Finder as "open"
        if cfg!(target_os = "macos") {
            flags |= gio::ApplicationFlags::HANDLES_OPEN;
        }
        let app: Self = glib::Object::builder()
            .property("application-id", env!("TRG_APPLICATION_ID"))
            .property("flags", flags)
            .build();
        *app.imp().client.borrow_mut() = Some(client.clone());
        app
    }

    fn main_window(&self) -> Option<MainWindow> {
        self.windows().into_iter().find_map(|w| w.downcast::<MainWindow>().ok())
    }

    /// The focused window, which can be a dialog (the "window." actions in
    /// the menu bar would always act on the main window, the only one that
    /// belongs to the application).
    fn focused_window(&self) -> Option<gtk::Window> {
        let toplevels = gtk::Window::toplevels();
        (0..toplevels.n_items())
            .filter_map(|i| toplevels.item(i).and_downcast::<gtk::Window>())
            .find(|w| w.is_active())
            .or_else(|| self.main_window().map(|w| w.upcast()))
    }

    fn add_simple_action(&self, name: &str, f: impl Fn(&Self) + 'static) {
        let action = gio::SimpleAction::new(name, None);
        let weak = self.downgrade();
        action.connect_activate(move |_, _| {
            if let Some(app) = weak.upgrade() {
                f(&app);
            }
        });
        self.add_action(&action);
    }

    /// macOS conventions: the menu in the system menu bar, with About and
    /// Preferences in the application menu, window actions, and following
    /// dark mode.
    fn macos_startup(&self, window: &MainWindow) {
        self.add_simple_action("about", |app| {
            if let Some(w) = app.main_window() {
                let _ = WidgetExt::activate_action(&w, "win.about", None);
            }
        });
        self.add_simple_action("preferences", |app| {
            if let Some(w) = app.main_window() {
                let _ = WidgetExt::activate_action(&w, "win.local-prefs", None);
            }
        });
        self.add_simple_action("close-window", |app| {
            if let Some(w) = app.focused_window() {
                w.close();
            }
        });
        self.add_simple_action("minimize", |app| {
            if let Some(w) = app.focused_window() {
                w.minimize();
            }
        });
        self.add_simple_action("zoom", |app| {
            if let Some(w) = app.focused_window() {
                if w.is_maximized() {
                    w.unmaximize();
                } else {
                    w.maximize();
                }
            }
        });

        self.set_menubar(Some(&window.menu_model()));
        crate::macos::follow_appearance();
    }

    fn startup_main(&self) {
        // "app.quit" (File -> Quit, Ctrl+Q) closes the window, rather than
        // quitting straight away, so that it can save its state.
        self.add_simple_action("quit", |app| match app.windows().first() {
            Some(w) => w.close(),
            None => app.quit(),
        });

        let client = self.imp().client.borrow().clone().unwrap();
        let window = MainWindow::new(self.upcast_ref(), &client);
        menus::set_accels(self.upcast_ref());
        if util::is_macos() {
            self.macos_startup(&window);
        }
        window.present();
        #[cfg(feature = "debug-snapshot")]
        crate::debug_snapshot::init(self.upcast_ref());
    }

    fn handle_command_line(&self, cmdline: &gio::ApplicationCommandLine) -> glib::ExitCode {
        let Some(window) = self.main_window() else {
            return glib::ExitCode::FAILURE;
        };

        // without the program name, and relative paths of files absolute
        let cwd = cmdline.cwd();
        let args: Vec<String> = cmdline
            .arguments()
            .into_iter()
            .skip(1)
            .map(|a| {
                let a = a.to_string_lossy().into_owned();
                let path = std::path::Path::new(&a);
                if !util::is_url(&a) && !util::is_magnet(&a) && path.is_relative() {
                    if let Some(cwd) = &cwd {
                        let full = cwd.join(path);
                        if full.is_file() {
                            return full.to_string_lossy().into_owned();
                        }
                    }
                }
                a
            })
            .collect();

        if cmdline.is_remote() {
            if args.is_empty() {
                window.present();
            } else {
                window.add_from_filenames(args);
            }
        } else {
            window.set_start_args(args);
            window.auto_connect_if_required();
        }

        glib::ExitCode::SUCCESS
    }
}
