//! The general panel (the first page of the notebook, trg-general-panel.c)
//! and the status bar (trg-status-bar.c).

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::client::Client;
use crate::dialog;
use crate::i18n::{sprintf, tr};
use crate::item::Item;
use crate::main_window::MainWindow;
use crate::prefs::{self, value_as_i64};
use crate::protocol::{self as p, Obj, get_bool, get_i64, get_str};
use crate::torrent_model::{self as tm, Stats};
use crate::util;

mod gp_imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-general-panel.ui")]
    pub struct GeneralPanel {
        #[template_child]
        pub gen_name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_size_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_completed_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_seeders_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_leechers_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_status_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_priority_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_eta_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_downloaded_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_uploaded_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_corrupted_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_down_rate_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_up_rate_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_ratio_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_limit_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_completedat_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_downloaddir_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_comment_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_hash_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_error_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub error_key_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GeneralPanel {
        const NAME: &'static str = "TrgGeneralPanel";
        type Type = super::GeneralPanel;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for GeneralPanel {
        fn constructed(&self) {
            self.parent_constructed();
            // The theme pads flow box children (room for their selection and
            // focus), which would indent the groups from the keys around them
            let css = gtk::CssProvider::new();
            css.load_from_string("flowbox.general-groups > flowboxchild { padding: 0; }");
            gtk::style_context_add_provider_for_display(
                &self.obj().display(),
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }
    impl WidgetImpl for GeneralPanel {}
    impl BoxImpl for GeneralPanel {}
}

glib::wrapper! {
    pub struct GeneralPanel(ObjectSubclass<gp_imp::GeneralPanel>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl GeneralPanel {
    fn labels(&self) -> Vec<gtk::Label> {
        let i = self.imp();
        vec![
            i.gen_name_label.get(),
            i.gen_size_label.get(),
            i.gen_completed_label.get(),
            i.gen_seeders_label.get(),
            i.gen_leechers_label.get(),
            i.gen_status_label.get(),
            i.gen_priority_label.get(),
            i.gen_eta_label.get(),
            i.gen_downloaded_label.get(),
            i.gen_uploaded_label.get(),
            i.gen_corrupted_label.get(),
            i.gen_down_rate_label.get(),
            i.gen_up_rate_label.get(),
            i.gen_ratio_label.get(),
            i.gen_limit_label.get(),
            i.gen_completedat_label.get(),
            i.gen_downloaddir_label.get(),
            i.gen_comment_label.get(),
            i.gen_hash_label.get(),
            i.gen_error_label.get(),
            i.error_key_label.get(),
        ]
    }

    pub fn clear(&self) {
        for l in self.labels() {
            l.set_text("");
        }
    }

    pub fn update(&self, t: &Obj, item: &Item) {
        let i = self.imp();
        let seeders = item.int64(tm::COL_SEEDS);
        let leechers = item.int64(tm::COL_LEECHERS);
        let status = item.string(tm::COL_STATUS).unwrap_or_default();

        i.gen_size_label.set_text(&util::format_size(get_i64(t, p::FIELD_SIZEWHENDONE)));

        let rate = |rate: i64, limited: &str, limit: &str| {
            let s = util::format_speed(rate);
            if get_bool(t, limited) {
                format!("{s} [{}]", util::format_speed_kbps(get_i64(t, limit) as f64))
            } else {
                s
            }
        };
        i.gen_down_rate_label.set_text(&rate(
            get_i64(t, p::FIELD_RATEDOWNLOAD),
            p::FIELD_DOWNLOAD_LIMITED,
            p::FIELD_DOWNLOAD_LIMIT,
        ));
        i.gen_up_rate_label.set_text(&rate(
            get_i64(t, p::FIELD_RATEUPLOAD),
            p::FIELD_UPLOAD_LIMITED,
            p::FIELD_UPLOAD_LIMIT,
        ));

        i.gen_corrupted_label.set_text(&util::format_size(get_i64(t, p::FIELD_CORRUPTEVER)));
        let uploaded = get_i64(t, p::FIELD_UPLOADEDEVER);
        i.gen_uploaded_label.set_text(&util::format_size(uploaded));
        i.gen_hash_label.set_text(get_str(t, p::FIELD_HASH_STRING).unwrap_or(""));

        let have_valid = get_i64(t, p::FIELD_HAVEVALID);
        i.gen_downloaded_label.set_text(&util::format_size(get_i64(t, p::FIELD_DOWNLOADEDEVER)));

        if uploaded > 0 && have_valid > 0 {
            i.gen_ratio_label.set_text(&util::format_ratio(uploaded as f64 / have_valid as f64));
        } else {
            i.gen_ratio_label.set_text(&tr("N/A"));
        }

        i.gen_limit_label.set_text(&util::format_ratio(p::get_f64(t, p::FIELD_SEED_RATIO_LIMIT)));

        let completed_at = get_i64(t, p::FIELD_DONE_DATE);
        i.gen_completedat_label.set_text(&if completed_at > 0 {
            util::epoch_to_string(completed_at)
        } else {
            String::new()
        });

        let privacy =
            if get_bool(t, p::FIELD_ISPRIVATE) { tr("(Private)") } else { tr("(Public)") };
        i.gen_status_label.set_text(&format!("{status} {privacy}"));

        match get_i64(t, p::FIELD_BANDWIDTH_PRIORITY) {
            p::TR_PRI_LOW => i.gen_priority_label.set_text(&tr("Low")),
            p::TR_PRI_NORMAL => i.gen_priority_label.set_text(&tr("Normal")),
            p::TR_PRI_HIGH => i.gen_priority_label.set_text(&tr("High")),
            _ => {}
        }

        i.gen_completed_label.set_text(&util::format_percent(p::torrent_get_percent_done(t)));
        i.gen_name_label.set_text(p::torrent_get_name(t));
        i.gen_downloaddir_label.set_text(get_str(t, p::FIELD_DOWNLOAD_DIR).unwrap_or(""));
        i.gen_comment_label
            .set_markup(&util::add_links_to_text(get_str(t, p::FIELD_COMMENT).unwrap_or("")));

        let error = get_str(t, p::FIELD_ERROR_STRING).unwrap_or("");
        if !error.is_empty() {
            i.gen_error_label.set_markup(&format!(
                "<span fgcolor=\"red\">{}</span>",
                glib::markup_escape_text(error)
            ));
            i.error_key_label.set_markup(&format!(
                "<span font_weight=\"bold\" fgcolor=\"red\">{}</span>",
                glib::markup_escape_text(&tr("Error"))
            ));
        } else {
            i.gen_error_label.set_text("");
            i.error_key_label.set_text("");
        }

        let eta = get_i64(t, p::FIELD_ETA);
        i.gen_eta_label.set_text(&if eta > 0 { util::format_time_long(eta) } else { tr("N/A") });

        i.gen_seeders_label.set_text(&seeders.max(0).to_string());
        i.gen_leechers_label.set_text(&leechers.max(0).to_string());
    }
}

/* The status bar: the connection status on the left, the free space and the
 * speeds (with the limits) on the right, and the alternative speed toggle. */

mod sb_imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-status-bar.ui")]
    pub struct StatusBar {
        #[template_child]
        pub speed_lbl: TemplateChild<gtk::Label>,
        #[template_child]
        pub turtle_image: TemplateChild<gtk::Image>,
        #[template_child]
        pub free_lbl: TemplateChild<gtk::Label>,
        #[template_child]
        pub info_lbl: TemplateChild<gtk::Label>,
        pub client: RefCell<Option<Client>>,
        pub win: glib::WeakRef<MainWindow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StatusBar {
        const NAME: &'static str = "TrgStatusBar";
        type Type = super::StatusBar;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for StatusBar {}
    impl WidgetImpl for StatusBar {}
    impl BoxImpl for StatusBar {}
}

glib::wrapper! {
    pub struct StatusBar(ObjectSubclass<sb_imp::StatusBar>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

#[gtk::template_callbacks]
impl StatusBar {
    #[template_callback]
    fn turtle_toggle(gesture: &gtk::GestureClick, _n_press: i32, _x: f64, _y: f64) {
        let Some(sb) = gesture.widget().and_then(|w| dialog::ancestor::<StatusBar>(&w)) else {
            return;
        };
        let imp = sb.imp();
        let alt_speed_on = imp.turtle_image.icon_name().as_deref() == Some("alt-speed-on");
        imp.turtle_image.set_icon_name(Some(if alt_speed_on {
            "alt-speed-off"
        } else {
            "alt-speed-on"
        }));

        let req = p::session_set().with(p::SGET_ALT_SPEED_ENABLED, !alt_speed_on);
        if let Some(win) = imp.win.upgrade() {
            win.dispatch_session_set(req);
        }
    }

    pub fn clear_indicators(&self) {
        self.imp().free_lbl.set_text("");
        self.imp().speed_lbl.set_text("");
    }

    pub fn reset(&self) {
        self.clear_indicators();
        self.imp().info_lbl.set_text(&tr("Disconnected"));
        self.imp().turtle_image.set_visible(false);
    }

    pub fn push_connection_msg(&self, msg: &str) {
        self.imp().info_lbl.set_text(msg);
    }

    fn set_connected_label(&self, session: &Obj, client: &Client) {
        let profile_name = client
            .prefs()
            .get_string(prefs::KEY_PROFILE_NAME, prefs::CONNECTION)
            .unwrap_or_default();
        self.push_connection_msg(&sprintf(
            &tr("Connected: %s :: %s"),
            &[&profile_name, &get_str(session, p::SGET_VERSION).unwrap_or("")],
        ));
    }

    pub fn connect(&self, session: &Obj, client: &Client) {
        self.set_connected_label(session, client);
        self.imp().speed_lbl.set_text(&tr("Updating torrents..."));
    }

    pub fn session_update(&self, session: &Obj) {
        let imp = self.imp();
        let free = session.get(p::SGET_DOWNLOAD_DIR_FREE_SPACE).map(value_as_i64).unwrap_or(-1);
        let alt = get_bool(session, p::SGET_ALT_SPEED_ENABLED);

        if free >= 0 {
            imp.free_lbl.set_text(&sprintf(&tr("Free space: %s"), &[&util::format_size(free)]));
        } else {
            imp.free_lbl.set_text("");
        }

        imp.turtle_image.set_icon_name(Some(if alt { "alt-speed-on" } else { "alt-speed-off" }));
        imp.turtle_image.set_tooltip_text(Some(&if alt {
            tr("Disable alternate speed limits")
        } else {
            tr("Enable alternate speed limits")
        }));
        imp.turtle_image.set_visible(true);
    }

    pub fn update_speed(&self, stats: &Stats, client: &Client) {
        let Some(session) = client.session_clone() else { return };
        let alt = get_bool(&session, p::SGET_ALT_SPEED_ENABLED);

        let (down_limit, up_limit) = if alt {
            (get_i64(&session, p::SGET_ALT_SPEED_DOWN), get_i64(&session, p::SGET_ALT_SPEED_UP))
        } else {
            (
                if get_bool(&session, p::SGET_SPEED_LIMIT_DOWN_ENABLED) {
                    get_i64(&session, p::SGET_SPEED_LIMIT_DOWN)
                } else {
                    -1
                },
                if get_bool(&session, p::SGET_SPEED_LIMIT_UP_ENABLED) {
                    get_i64(&session, p::SGET_SPEED_LIMIT_UP)
                } else {
                    -1
                },
            )
        };

        let limit = |l: i64| {
            if l >= 0 {
                sprintf(&tr(" (Limit: %s)"), &[&util::format_speed_kbps(l as f64)])
            } else {
                String::new()
            }
        };

        let text = sprintf(
            &tr("Down: %s%s, Up: %s%s"),
            &[
                &util::format_speed(stats.down_rate_total),
                &limit(down_limit),
                &util::format_speed(stats.up_rate_total),
                &limit(up_limit),
            ],
        );
        self.imp().speed_lbl.set_text(&text);
    }

    pub fn update(&self, stats: &Stats, client: &Client) {
        if let Some(session) = client.session_clone() {
            self.set_connected_label(&session, client);
        }
        self.update_speed(stats, client);
    }

    pub fn speed_text(&self) -> String {
        self.imp().speed_lbl.text().to_string()
    }

    /// The main window has the status bar in its template, so this is set later
    pub fn set_client(&self, win: &MainWindow, client: &Client) {
        *self.imp().client.borrow_mut() = Some(client.clone());
        self.imp().win.set(Some(win));
    }
}
