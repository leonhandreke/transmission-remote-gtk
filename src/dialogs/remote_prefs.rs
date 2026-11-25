//! The settings of the server: loaded from the latest session object of the
//! client into a settings object, which the widgets are bound to; the
//! settings changed are sent with a session-set on OK. The session is then
//! refreshed (see Client::dispatch_session_set).

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::client::Client;
use crate::dialogs::dialog::{Dialog, DialogImpl, Response, TrgDialogExt};
use crate::dialogs::settings::{Closures, settings_object, source_property};
use crate::i18n::{sprintf, tr};
use crate::main_window::MainWindow;
use crate::protocol::{self as p, Session};
use crate::util;

settings_object! {
    /// The settings of the server the dialog shows, the fields of the session
    pub struct RemoteSettings("TrgRemoteSettings", Session) in mod settings_imp {
        download_dir: String,
        incomplete_dir_enabled: bool,
        incomplete_dir: String,
        script_torrent_done_enabled: bool,
        script_torrent_done_filename: String,
        cache_size_mib: i64,
        rename_partial_files: bool,
        trash_original_torrent_files: bool,
        start_added_torrents: bool,
        sequential_download: Option<bool>,

        peer_port: i64,
        peer_port_random_on_start: bool,
        port_forwarding_enabled: bool,
        encryption: String,
        preferred_transports: Option<Vec<String>>,
        pex_enabled: bool,
        dht_enabled: bool,
        lpd_enabled: bool,
        blocklist_enabled: bool,
        blocklist_url: String,

        speed_limit_down_enabled: bool,
        speed_limit_down: i64,
        speed_limit_up_enabled: bool,
        speed_limit_up: i64,
        alt_speed_enabled: bool,
        alt_speed_time_enabled: bool,
        alt_speed_time_begin: i64,
        alt_speed_time_end: i64,
        alt_speed_time_day: i64,
        alt_speed_down: i64,
        alt_speed_up: i64,

        seed_ratio_limited: bool,
        seed_ratio_limit: f64,
        download_queue_enabled: bool,
        download_queue_size: i64,
        seed_queue_enabled: bool,
        seed_queue_size: i64,
        queue_stalled_enabled: bool,
        queue_stalled_minutes: i64,
        peer_limit_global: i64,
        peer_limit_per_torrent: i64,
    }
}

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/remote-prefs-dialog.ui")]
    pub struct RemotePrefsDialog {
        #[template_child]
        pub settings: TemplateChild<RemoteSettings>,
        #[template_child]
        pub encryption_combo: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub transport_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub transport_combo: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub port_test_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub port_test_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub blocklist_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub blocklist_update_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub alt_begin_hours_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub alt_begin_mins_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub alt_end_hours_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub alt_end_mins_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub alt_days_grid: TemplateChild<gtk::Grid>,

        pub client: RefCell<Option<Client>>,
        pub win: glib::WeakRef<MainWindow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RemotePrefsDialog {
        const NAME: &'static str = "TrgRemotePrefsDialog";
        type Type = super::RemotePrefsDialog;
        type ParentType = Dialog;

        fn class_init(klass: &mut Self::Class) {
            RemoteSettings::ensure_type();
            klass.bind_template();
            klass.bind_template_instance_callbacks();
            Closures::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for RemotePrefsDialog {}
    impl WidgetImpl for RemotePrefsDialog {}
    impl WindowImpl for RemotePrefsDialog {}
    impl DialogImpl for RemotePrefsDialog {}
}

glib::wrapper! {
    pub struct RemotePrefsDialog(ObjectSubclass<imp::RemotePrefsDialog>)
        @extends Dialog, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root,
            gtk::ShortcutManager;
}

fn dlg(w: &impl IsA<gtk::Widget>) -> Option<RemotePrefsDialog> {
    util::ancestor::<RemotePrefsDialog>(w)
}

/// The abbreviated names of the days of the week of the locale (Sunday
/// first), rather than the translations
#[cfg(unix)]
fn locale_day_names() -> Option<Vec<String>> {
    let items = [
        libc::ABDAY_1,
        libc::ABDAY_2,
        libc::ABDAY_3,
        libc::ABDAY_4,
        libc::ABDAY_5,
        libc::ABDAY_6,
        libc::ABDAY_7,
    ];
    items
        .iter()
        .map(|&item| {
            let s = unsafe { std::ffi::CStr::from_ptr(libc::nl_langinfo(item)) };
            s.to_str().ok().filter(|s| !s.is_empty()).map(str::to_string)
        })
        .collect()
}

#[cfg(not(unix))]
fn locale_day_names() -> Option<Vec<String>> {
    None
}

#[gtk::template_callbacks]
impl RemotePrefsDialog {
    /// Two digits
    #[template_callback]
    fn time_spin_output(spin: &gtk::SpinButton) -> bool {
        spin.set_text(&format!("{:02}", spin.value_as_int()));
        true
    }

    #[template_callback]
    fn port_test_cb(button: &gtk::Button) {
        let Some(d) = dlg(button) else { return };
        let imp = d.imp();
        imp.port_test_label.set_text(&tr("Port test"));
        button.set_label(&tr("Testing..."));
        button.set_sensitive(false);

        // IPv4 and IPv6 separately, in one go
        let families = [(p::IP_PROTOCOL_IPV4, "IPv4"), (p::IP_PROTOCOL_IPV6, "IPv6")];
        let reqs = families.iter().map(|(ip, _)| p::port_test(Some(ip))).collect();
        let weak = d.downgrade();
        d.client().dispatch_batch(reqs, move |results| {
            let Some(d) = weak.upgrade() else { return };
            let imp = d.imp();
            imp.port_test_button.set_label(&tr("Retest"));
            imp.port_test_button.set_sensitive(true);

            // couldn't reach the server
            if let Some(Err(e)) = results.iter().find(|r| matches!(r, Err(e) if e.is_transport())) {
                util::client_error_dialog(Some(&d), e);
                return;
            }

            let mut markup = Vec::new();
            let mut errors = Vec::new();
            for ((_, name), result) in families.iter().zip(&results) {
                let state = match result {
                    Ok(r) if p::parse::<p::PortTest>(r).port_is_open => {
                        format!(
                            "<span font_weight=\"bold\" fgcolor=\"darkgreen\">{}</span>",
                            tr("open")
                        )
                    }
                    Ok(_) => format!(
                        "<span font_weight=\"bold\" fgcolor=\"red\">{}</span>",
                        tr("closed")
                    ),
                    // eg. no IPv6 connectivity
                    Err(e) => {
                        errors.push(format!("{name}: {e}"));
                        tr("unavailable")
                    }
                };
                markup.push(format!("{name}: {state}"));
            }
            imp.port_test_label.set_markup(&markup.join("\n"));
            imp.port_test_label
                .set_tooltip_text((!errors.is_empty()).then(|| errors.join("\n")).as_deref());
        });
    }

    #[template_callback]
    fn update_blocklist_cb(button: &gtk::Button) {
        let Some(d) = dlg(button) else { return };
        button.set_sensitive(false);
        button.set_label(&tr("Updating..."));

        let weak = d.downgrade();
        d.client().dispatch(p::blocklist_update(), move |response| {
            let Some(d) = weak.upgrade() else { return };
            let imp = d.imp();
            imp.blocklist_update_button.set_sensitive(true);
            imp.blocklist_update_button.set_label(&tr("Update"));
            match response {
                Ok(r) => {
                    let size = p::parse::<p::BlocklistUpdate>(&r).blocklist_size;
                    imp.blocklist_check
                        .set_label(Some(&sprintf(&tr("Blocklist (%ld entries)"), &[&size])));
                }
                Err(e) => util::client_error_dialog(Some(&d), &e),
            }
        });
    }

    pub fn new(win: &MainWindow, client: &Client) -> Self {
        let d: Self = glib::Object::new();
        let imp = d.imp();
        *imp.client.borrow_mut() = Some(client.clone());
        imp.win.set(Some(win));
        d.set_transient_for(Some(win));

        let session = client.session_clone().unwrap_or_default();
        imp.transport_label.set_visible(session.preferred_transports.is_some());
        imp.transport_combo.set_visible(session.preferred_transports.is_some());
        imp.blocklist_check
            .set_label(Some(&sprintf(&tr("Blocklist (%ld entries)"), &[&session.blocklist_size])));
        imp.settings.load(session);

        d.bind_encryption();
        d.bind_transports();
        d.bind_time("alt-speed-time-begin", &imp.alt_begin_hours_spin, &imp.alt_begin_mins_spin);
        d.bind_time("alt-speed-time-end", &imp.alt_end_hours_spin, &imp.alt_end_mins_spin);
        d.bind_days();

        d.on_response(|d, response| {
            if response == Response::Ok {
                d.update_session();
            }
            d.destroy();
        });

        d
    }

    fn client(&self) -> Client {
        self.imp().client.borrow().clone().unwrap()
    }

    /// The encryption, in the order of the drop-down
    fn bind_encryption(&self) {
        let imp = self.imp();
        imp.settings
            .bind_property("encryption", &*imp.encryption_combo, "selected")
            .transform_to(|_, encryption: String| {
                Some(match encryption.as_str() {
                    p::ENCRYPTION_REQUIRED => 0u32,
                    p::ENCRYPTION_ALLOWED => 2,
                    _ => 1,
                })
            })
            .transform_from(|_, selected: u32| {
                Some(match selected {
                    0 => p::ENCRYPTION_REQUIRED,
                    2 => p::ENCRYPTION_ALLOWED,
                    _ => p::ENCRYPTION_PREFERRED,
                })
            })
            .bidirectional()
            .sync_create()
            .build();
    }

    /// The preferred transports, in the order of the transport drop-down
    const TRANSPORTS: [&'static [&'static str]; 4] = [
        &[p::TRANSPORT_UTP, p::TRANSPORT_TCP],
        &[p::TRANSPORT_TCP, p::TRANSPORT_UTP],
        &[p::TRANSPORT_TCP],
        &[p::TRANSPORT_UTP],
    ];

    /// The preferred transports, a position of the drop-down (the first if
    /// they're none of them)
    fn bind_transports(&self) {
        let imp = self.imp();
        imp.settings
            .bind_property("preferred-transports", &*imp.transport_combo, "selected")
            .transform_to(|_, transports: Vec<String>| {
                let position = Self::TRANSPORTS.iter().position(|t| *t == transports.as_slice());
                Some(position.unwrap_or(0) as u32)
            })
            .transform_from(|_, selected: u32| {
                let transports =
                    Self::TRANSPORTS.get(selected as usize).unwrap_or(&Self::TRANSPORTS[0]);
                Some(transports.iter().map(|t| t.to_string()).collect::<Vec<String>>())
            })
            .bidirectional()
            .sync_create()
            .build();
    }

    /// A time of day in minutes (a property of the settings), in a spin
    /// button for the hours and one for the minutes
    fn bind_time(&self, property: &'static str, hours: &gtk::SpinButton, mins: &gtk::SpinButton) {
        let settings = &*self.imp().settings;
        settings
            .bind_property(property, hours, "value")
            .transform_to(|_, time: i64| Some((time / 60) as f64))
            .transform_from(move |b, h: f64| {
                Some(h.round() as i64 * 60 + source_property::<i64>(b, property) % 60)
            })
            .bidirectional()
            .sync_create()
            .build();
        settings
            .bind_property(property, mins, "value")
            .transform_to(|_, time: i64| Some((time % 60) as f64))
            .transform_from(move |b, m: f64| {
                Some(source_property::<i64>(b, property) / 60 * 60 + m.round() as i64)
            })
            .bidirectional()
            .sync_create()
            .build();
    }

    /// The days of the week of the alternate times, a bit each (Sunday
    /// first), in the check boxes of the grid
    fn bind_days(&self) {
        let imp = self.imp();
        let names = if cfg!(feature = "nl_langinfo") { locale_day_names() } else { None };
        for i in 0..7 {
            let Some(check) = imp.alt_days_grid.child_at(i, 0).and_downcast::<gtk::CheckButton>()
            else {
                continue;
            };
            if let Some(names) = &names {
                check.set_label(Some(&names[i as usize]));
            }
            let bit = 1i64 << i;
            imp.settings
                .bind_property("alt-speed-time-day", &check, "active")
                .transform_to(move |_, days: i64| Some(days & bit != 0))
                .transform_from(move |b, active: bool| {
                    let days = source_property::<i64>(b, "alt-speed-time-day");
                    Some(if active { days | bit } else { days & !bit })
                })
                .bidirectional()
                .sync_create()
                .build();
        }
    }

    /// Sends the settings that changed, if any
    fn update_session(&self) {
        let changes = self.imp().settings.changes();
        if changes.is_empty() {
            return;
        }
        let mut req = p::session_set();
        req.params_mut().extend(changes);
        self.client().dispatch_session_set(req);
    }
}
