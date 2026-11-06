//! The statistics of the server (trg-stats-dialog.c), refreshed while the
//! dialog is open.

use std::cell::{Cell, RefCell};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::client::{Client, Response};
use crate::dialog::{Dialog, DialogImpl, TrgDialogExt};
use crate::main_window::MainWindow;
use crate::protocol::{self as p, Obj, get_f64, get_i64};
use crate::util;

const STATS_UPDATE_INTERVAL: u32 = 5;
/// A request that got no reply within this many intervals is given up on (a
/// reply is dropped when the client disconnects in between).
const STATS_STALE_TICKS: u32 = 6;

thread_local! {
    static INSTANCE: RefCell<glib::WeakRef<StatsDialog>> = RefCell::new(glib::WeakRef::new());
    /// Identifies the dialog a reply is for, so a reply that arrives after
    /// the dialog was closed (or for an earlier one) is ignored.
    static GENERATION: Cell<u32> = const { Cell::new(0) };
}

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-stats-dialog.ui")]
    pub struct StatsDialog {
        #[template_child]
        pub grid: TemplateChild<gtk::Grid>,
        #[template_child]
        pub version_session: TemplateChild<gtk::Label>,
        #[template_child]
        pub down_session: TemplateChild<gtk::Label>,
        #[template_child]
        pub down_cumulat: TemplateChild<gtk::Label>,
        #[template_child]
        pub up_session: TemplateChild<gtk::Label>,
        #[template_child]
        pub up_cumulat: TemplateChild<gtk::Label>,
        #[template_child]
        pub ratio_session: TemplateChild<gtk::Label>,
        #[template_child]
        pub ratio_cumulat: TemplateChild<gtk::Label>,
        #[template_child]
        pub files_added_session: TemplateChild<gtk::Label>,
        #[template_child]
        pub files_added_cumulat: TemplateChild<gtk::Label>,
        #[template_child]
        pub session_count_session: TemplateChild<gtk::Label>,
        #[template_child]
        pub session_count_cumulat: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_session: TemplateChild<gtk::Label>,
        #[template_child]
        pub active_cumulat: TemplateChild<gtk::Label>,

        pub client: RefCell<Option<Client>>,
        pub timer: RefCell<Option<glib::SourceId>>,
        pub generation: Cell<u32>,
        /// 0: no request pending
        pub in_flight_ticks: Cell<u32>,
        pub error_shown: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StatsDialog {
        const NAME: &'static str = "TrgStatsDialog";
        type Type = super::StatsDialog;
        type ParentType = Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for StatsDialog {
        fn dispose(&self) {
            // Also when destroyed with the main window, without a response.
            if let Some(id) = self.timer.take() {
                id.remove();
            }
        }
    }

    impl WidgetImpl for StatsDialog {}
    impl WindowImpl for StatsDialog {}
    impl DialogImpl for StatsDialog {}
}

glib::wrapper! {
    pub struct StatsDialog(ObjectSubclass<imp::StatsDialog>)
        @extends Dialog, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root,
            gtk::ShortcutManager;
}

pub fn instance(win: &MainWindow, client: &Client) -> StatsDialog {
    if let Some(d) = INSTANCE.with(|i| i.borrow().upgrade()) {
        return d;
    }
    let d = StatsDialog::new(win, client);
    INSTANCE.with(|i| i.borrow().set(Some(&d)));
    d
}

fn stats<'a>(args: &'a Obj, key: &str) -> Option<&'a Obj> {
    p::get_obj(args, key)
}

impl StatsDialog {
    fn new(win: &MainWindow, client: &Client) -> Self {
        let d: Self = glib::Object::new();
        let imp = d.imp();
        *imp.client.borrow_mut() = Some(client.clone());

        d.set_transient_for(Some(win));
        d.on_response(|d, _| d.destroy());

        let generation = GENERATION.with(|g| {
            g.set(g.get() + 1);
            g.get()
        });
        imp.generation.set(generation);
        d.request();

        let weak = d.downgrade();
        let id = glib::timeout_add_seconds_local(STATS_UPDATE_INTERVAL, move || {
            let Some(d) = weak.upgrade() else { return glib::ControlFlow::Break };
            d.tick();
            glib::ControlFlow::Continue
        });
        *imp.timer.borrow_mut() = Some(id);

        d
    }

    fn client(&self) -> Client {
        self.imp().client.borrow().clone().unwrap()
    }

    fn request(&self) {
        let imp = self.imp();
        imp.in_flight_ticks.set(1);
        let generation = imp.generation.get();
        self.client().dispatch(p::session_stats(), move |response| {
            if let Some(d) = INSTANCE.with(|i| i.borrow().upgrade()) {
                if d.imp().generation.get() == generation {
                    d.on_stats_reply(response);
                }
            }
        });
    }

    /// Runs as long as the dialog is open, so the refresh resumes after a
    /// reconnect.
    fn tick(&self) {
        let imp = self.imp();
        let connected = self.client().is_connected();
        imp.grid.set_sensitive(connected);

        if !connected {
            imp.in_flight_ticks.set(0);
        } else if imp.in_flight_ticks.get() == 0 || imp.in_flight_ticks.get() >= STATS_STALE_TICKS {
            self.request();
        } else {
            imp.in_flight_ticks.set(imp.in_flight_ticks.get() + 1);
        }
    }

    fn on_stats_reply(&self, response: Response) {
        let imp = self.imp();
        imp.in_flight_ticks.set(0);

        match response {
            Ok(args) => self.show_stats(&args),
            // The main window reports (and retries) failed connections.
            // Other errors once until the next successful update, not on
            // every refresh.
            Err(e)
                if !e.is_transport() && !imp.error_shown.get() && self.client().is_connected() =>
            {
                imp.error_shown.set(true);
                util::client_error_dialog(Some(self), &e);
            }
            Err(_) => {}
        }
    }

    fn show_stats(&self, args: &Obj) {
        let imp = self.imp();
        {
            let empty = Obj::new();
            let session = stats(args, p::STATS_CURRENT).unwrap_or(&empty);
            let cumulat = stats(args, p::STATS_CUMULATIVE).unwrap_or(&empty);

            imp.version_session.set_text(&format!(
                "Transmission {}",
                self.client().version_string().unwrap_or_default()
            ));

            let set = |s: &gtk::Label, c: &gtk::Label, f: &dyn Fn(&Obj) -> String| {
                s.set_text(&f(session));
                c.set_text(&f(cumulat));
            };
            set(&imp.up_session, &imp.up_cumulat, &|o| {
                util::format_size(get_i64(o, p::STATS_UPLOADED_BYTES))
            });
            set(&imp.down_session, &imp.down_cumulat, &|o| {
                util::format_size(get_i64(o, p::STATS_DOWNLOADED_BYTES))
            });
            // NAN and INFINITY are handled by format_ratio
            set(&imp.ratio_session, &imp.ratio_cumulat, &|o| {
                util::format_ratio(
                    get_f64(o, p::STATS_UPLOADED_BYTES) / get_f64(o, p::STATS_DOWNLOADED_BYTES),
                )
            });
            set(&imp.files_added_session, &imp.files_added_cumulat, &|o| {
                get_i64(o, p::STATS_FILES_ADDED).to_string()
            });
            set(&imp.session_count_session, &imp.session_count_cumulat, &|o| {
                get_i64(o, p::STATS_SESSION_COUNT).to_string()
            });
            set(&imp.active_session, &imp.active_cumulat, &|o| {
                util::format_time_long(get_i64(o, p::STATS_SECONDS_ACTIVE))
            });

            imp.error_shown.set(false);
        }
    }
}
