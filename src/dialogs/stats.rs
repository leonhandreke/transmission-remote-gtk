//! The statistics of the server, refreshed while the dialog is open.

use std::cell::{Cell, RefCell};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::client::{Client, Response};
use crate::dialogs::dialog::{Dialog, DialogImpl, TrgDialogExt};
use crate::format;
use crate::json::Obj;
use crate::main_window::MainWindow;
use crate::protocol as p;
use crate::util;

const STATS_UPDATE_INTERVAL: u32 = 5;
/// A request that got no reply within this many intervals is given up on (a
/// reply is dropped when the client disconnects in between).
const STATS_STALE_TICKS: u32 = 6;

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/stats-dialog.ui")]
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

impl StatsDialog {
    pub fn new(win: &MainWindow, client: &Client) -> Self {
        let d: Self = glib::Object::new();
        let imp = d.imp();
        *imp.client.borrow_mut() = Some(client.clone());

        d.set_transient_for(Some(win));
        d.on_response(|d, _| d.destroy());

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
        let weak = self.downgrade();
        self.client().dispatch(p::session_stats(), move |response| {
            if let Some(d) = weak.upgrade() {
                d.on_stats_reply(response);
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

    fn show_stats(&self, result: &Obj) {
        let imp = self.imp();
        let stats: p::SessionStats = p::parse(result);
        let (session, cumulative) = (&stats.current_stats, &stats.cumulative_stats);

        imp.version_session.set_text(&format!(
            "Transmission {}",
            self.client().version_string().unwrap_or_default()
        ));

        let set = |s: &gtk::Label, c: &gtk::Label, f: &dyn Fn(&p::TransferStats) -> String| {
            s.set_text(&f(session));
            c.set_text(&f(cumulative));
        };
        set(&imp.up_session, &imp.up_cumulat, &|s| format::size(s.uploaded_bytes));
        set(&imp.down_session, &imp.down_cumulat, &|s| format::size(s.downloaded_bytes));
        // NAN and INFINITY are handled by format::ratio()
        set(&imp.ratio_session, &imp.ratio_cumulat, &|s| {
            format::ratio(s.uploaded_bytes as f64 / s.downloaded_bytes as f64)
        });
        set(&imp.files_added_session, &imp.files_added_cumulat, &|s| s.files_added.to_string());
        set(&imp.session_count_session, &imp.session_count_cumulat, &|s| {
            s.session_count.to_string()
        });
        set(&imp.active_session, &imp.active_cumulat, &|s| format::time_long(s.seconds_active));

        imp.error_shown.set(false);
    }
}
