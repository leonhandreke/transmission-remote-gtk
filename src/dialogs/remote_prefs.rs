//! The settings of the server (trg-remote-prefs-dialog.c): loaded from the
//! latest session object of the client, sent with a session-set on OK. The
//! session is then refreshed (see MainWindow::on_session_set).

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};
use serde_json::json;

use crate::client::Client;
use crate::dialog::{self, Dialog, DialogImpl, TrgDialogExt};
use crate::dialogs::json_widgets::JsonWidgets;
use crate::i18n::{sprintf, tr};
use crate::main_window::MainWindow;
use crate::protocol::{self as p, Obj, get_bool, get_i64, get_str};
use crate::util;

thread_local! {
    static INSTANCE: RefCell<glib::WeakRef<RemotePrefsDialog>> = RefCell::new(glib::WeakRef::new());
}

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-remote-prefs-dialog.ui")]
    pub struct RemotePrefsDialog {
        #[template_child]
        pub encryption_combo: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub port_test_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub port_test_button: TemplateChild<gtk::Button>,
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
        #[template_child]
        pub cache_size_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub cache_size_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub blocklist_url_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub blocklist_url_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub queues_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub download_dir_entry: TemplateChild<gtk::Widget>,
        #[template_child]
        pub incomplete_dir_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub incomplete_dir_entry: TemplateChild<gtk::Widget>,
        #[template_child]
        pub script_torrent_done_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub script_torrent_done_entry: TemplateChild<gtk::Widget>,
        #[template_child]
        pub rename_partial_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub trash_original_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub start_added_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub peer_port_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub peer_port_random_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub port_forwarding_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub sequential_download_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub transport_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub transport_combo: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub pex_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub dht_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub lpd_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub blocklist_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub speed_limit_down_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub speed_limit_down_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub speed_limit_up_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub speed_limit_up_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub alt_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub alt_time_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub alt_speed_down_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub alt_speed_up_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub seed_ratio_limited_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub seed_ratio_limit_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub peer_limit_global_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub peer_limit_per_torrent_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub download_queue_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub download_queue_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub seed_queue_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub seed_queue_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub queue_stalled_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub queue_stalled_spin: TemplateChild<gtk::Widget>,

        pub client: RefCell<Option<Client>>,
        pub win: glib::WeakRef<MainWindow>,
        pub widgets: RefCell<JsonWidgets>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RemotePrefsDialog {
        const NAME: &'static str = "TrgRemotePrefsDialog";
        type Type = super::RemotePrefsDialog;
        type ParentType = Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
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

pub fn instance(win: &MainWindow, client: &Client) -> RemotePrefsDialog {
    if let Some(d) = INSTANCE.with(|i| i.borrow().upgrade()) {
        return d;
    }
    let d = RemotePrefsDialog::new(win, client);
    INSTANCE.with(|i| i.borrow().set(Some(&d)));
    d
}

fn dlg(w: &impl IsA<gtk::Widget>) -> Option<RemotePrefsDialog> {
    dialog::ancestor::<RemotePrefsDialog>(w)
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
    /// The alternate limits apply when they're on, or at the alternate times
    #[template_callback]
    fn alt_speed_dependency_cb(check: &gtk::CheckButton, _pspec: &glib::ParamSpec) {
        if let Some(d) = dlg(check) {
            d.alt_speed_dependency();
        }
    }

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
                    Ok(r) if get_bool(r, p::FIELD_PORT_IS_OPEN) => {
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
                    let size = get_i64(&r, p::SGET_BLOCKLIST_SIZE);
                    imp.blocklist_check
                        .set_label(Some(&sprintf(&tr("Blocklist (%ld entries)"), &[&size])));
                }
                Err(e) => util::client_error_dialog(Some(&d), &e),
            }
        });
    }

    fn new(win: &MainWindow, client: &Client) -> Self {
        let d: Self = glib::Object::new();
        let imp = d.imp();
        *imp.client.borrow_mut() = Some(client.clone());
        imp.win.set(Some(win));
        d.set_transient_for(Some(win));

        let s = client.session_clone().unwrap_or_default();
        let mut widgets = JsonWidgets::default();

        let session_widgets: Vec<(gtk::Widget, &str)> = vec![
            (imp.download_dir_entry.get().upcast::<gtk::Widget>(), p::SGET_DOWNLOAD_DIR),
            (
                imp.incomplete_dir_check.get().upcast::<gtk::Widget>(),
                p::SGET_INCOMPLETE_DIR_ENABLED,
            ),
            (imp.incomplete_dir_entry.get().upcast::<gtk::Widget>(), p::SGET_INCOMPLETE_DIR),
            (
                imp.script_torrent_done_check.get().upcast::<gtk::Widget>(),
                p::SGET_SCRIPT_TORRENT_DONE_ENABLED,
            ),
            (
                imp.script_torrent_done_entry.get().upcast::<gtk::Widget>(),
                p::SGET_SCRIPT_TORRENT_DONE_FILENAME,
            ),
            (imp.rename_partial_check.get().upcast::<gtk::Widget>(), p::SGET_RENAME_PARTIAL_FILES),
            (
                imp.trash_original_check.get().upcast::<gtk::Widget>(),
                p::SGET_TRASH_ORIGINAL_TORRENT_FILES,
            ),
            (imp.start_added_check.get().upcast::<gtk::Widget>(), p::SGET_START_ADDED_TORRENTS),
            (
                imp.sequential_download_check.get().upcast::<gtk::Widget>(),
                p::SGET_SEQUENTIAL_DOWNLOAD,
            ),
            (imp.peer_port_spin.get().upcast::<gtk::Widget>(), p::SGET_PEER_PORT),
            (
                imp.peer_port_random_check.get().upcast::<gtk::Widget>(),
                p::SGET_PEER_PORT_RANDOM_ON_START,
            ),
            (
                imp.port_forwarding_check.get().upcast::<gtk::Widget>(),
                p::SGET_PORT_FORWARDING_ENABLED,
            ),
            (imp.pex_check.get().upcast::<gtk::Widget>(), p::SGET_PEX_ENABLED),
            (imp.dht_check.get().upcast::<gtk::Widget>(), p::SGET_DHT_ENABLED),
            (imp.lpd_check.get().upcast::<gtk::Widget>(), p::SGET_LPD_ENABLED),
            (imp.blocklist_check.get().upcast::<gtk::Widget>(), p::SGET_BLOCKLIST_ENABLED),
            (
                imp.speed_limit_down_check.get().upcast::<gtk::Widget>(),
                p::SGET_SPEED_LIMIT_DOWN_ENABLED,
            ),
            (imp.speed_limit_down_spin.get().upcast::<gtk::Widget>(), p::SGET_SPEED_LIMIT_DOWN),
            (
                imp.speed_limit_up_check.get().upcast::<gtk::Widget>(),
                p::SGET_SPEED_LIMIT_UP_ENABLED,
            ),
            (imp.speed_limit_up_spin.get().upcast::<gtk::Widget>(), p::SGET_SPEED_LIMIT_UP),
            (imp.alt_check.get().upcast::<gtk::Widget>(), p::SGET_ALT_SPEED_ENABLED),
            (imp.alt_time_check.get().upcast::<gtk::Widget>(), p::SGET_ALT_SPEED_TIME_ENABLED),
            (imp.alt_speed_down_spin.get().upcast::<gtk::Widget>(), p::SGET_ALT_SPEED_DOWN),
            (imp.alt_speed_up_spin.get().upcast::<gtk::Widget>(), p::SGET_ALT_SPEED_UP),
            (
                imp.seed_ratio_limited_check.get().upcast::<gtk::Widget>(),
                p::SGET_SEED_RATIO_LIMITED,
            ),
            (imp.seed_ratio_limit_spin.get().upcast::<gtk::Widget>(), p::SGET_SEED_RATIO_LIMIT),
            (imp.peer_limit_global_spin.get().upcast::<gtk::Widget>(), p::SGET_PEER_LIMIT_GLOBAL),
            (
                imp.peer_limit_per_torrent_spin.get().upcast::<gtk::Widget>(),
                p::SGET_PEER_LIMIT_PER_TORRENT,
            ),
        ];
        for (w, key) in &session_widgets {
            widgets.add(w, &s, key);
        }

        let queue_widgets: Vec<(gtk::Widget, &str)> = vec![
            (
                imp.download_queue_check.get().upcast::<gtk::Widget>(),
                p::SGET_DOWNLOAD_QUEUE_ENABLED,
            ),
            (imp.download_queue_spin.get().upcast::<gtk::Widget>(), p::SGET_DOWNLOAD_QUEUE_SIZE),
            (imp.seed_queue_check.get().upcast::<gtk::Widget>(), p::SGET_SEED_QUEUE_ENABLED),
            (imp.seed_queue_spin.get().upcast::<gtk::Widget>(), p::SGET_SEED_QUEUE_SIZE),
            (imp.queue_stalled_check.get().upcast::<gtk::Widget>(), p::SGET_QUEUE_STALLED_ENABLED),
            (imp.queue_stalled_spin.get().upcast::<gtk::Widget>(), p::SGET_QUEUE_STALLED_MINUTES),
        ];
        if s.contains_key(p::SGET_DOWNLOAD_QUEUE_ENABLED) {
            for (w, key) in &queue_widgets {
                widgets.add(w, &s, key);
            }
        } else {
            imp.queues_label.set_visible(false);
            for (w, _) in &queue_widgets {
                w.set_visible(false);
            }
        }

        if p::session_get_cache_size_mib(&s) >= 0 {
            widgets.add(&imp.cache_size_spin.get(), &s, p::SGET_CACHE_SIZE_MIB);
        } else {
            imp.cache_size_label.set_visible(false);
            imp.cache_size_spin.set_visible(false);
        }

        if get_str(&s, p::SGET_BLOCKLIST_URL).is_some() {
            widgets.add(&imp.blocklist_url_entry.get(), &s, p::SGET_BLOCKLIST_URL);
        } else {
            imp.blocklist_url_label.set_visible(false);
            imp.blocklist_url_entry.set_visible(false);
        }

        Self::time_widget_add(
            &mut widgets,
            &s,
            p::SGET_ALT_SPEED_TIME_BEGIN,
            &imp.alt_begin_hours_spin,
            &imp.alt_begin_mins_spin,
        );
        Self::time_widget_add(
            &mut widgets,
            &s,
            p::SGET_ALT_SPEED_TIME_END,
            &imp.alt_end_hours_spin,
            &imp.alt_end_mins_spin,
        );
        d.alt_days_add(&mut widgets, &s, p::SGET_ALT_SPEED_TIME_DAY);
        *imp.widgets.borrow_mut() = widgets;
        d.alt_speed_dependency();

        let encryption = get_str(&s, p::SGET_ENCRYPTION).unwrap_or("");
        imp.encryption_combo.set_selected(match encryption {
            p::ENCRYPTION_REQUIRED => 0,
            p::ENCRYPTION_ALLOWED => 2,
            _ => 1,
        });

        d.transports_add(&mut imp.widgets.borrow_mut(), &s);

        imp.blocklist_check.set_label(Some(&sprintf(
            &tr("Blocklist (%ld entries)"),
            &[&get_i64(&s, p::SGET_BLOCKLIST_SIZE)],
        )));

        d.on_response(|d, response| {
            if response == dialog::RESPONSE_OK {
                d.update_session();
            }
            d.destroy();
        });

        d
    }

    fn client(&self) -> Client {
        self.imp().client.borrow().clone().unwrap()
    }

    fn alt_speed_dependency(&self) {
        let imp = self.imp();
        let sensitive = imp.alt_time_check.is_active() || imp.alt_check.is_active();
        imp.alt_speed_down_spin.set_sensitive(sensitive);
        imp.alt_speed_up_spin.set_sensitive(sensitive);
    }

    /// A time of day in minutes, in a spin button for the hours and one for
    /// the minutes
    fn time_widget_add(
        widgets: &mut JsonWidgets,
        obj: &Obj,
        key: &str,
        hours: &gtk::SpinButton,
        mins: &gtk::SpinButton,
    ) {
        let value = get_i64(obj, key);
        hours.set_value((value as f64 / 60.0).floor());
        mins.set_value((value % 60) as f64);
        let (h, m) = (hours.clone(), mins.clone());
        widgets
            .add_custom(key, move || json!(h.value_as_int() as i64 * 60 + m.value_as_int() as i64));
    }

    /// The preferred transports, in the order of the transport combo
    const TRANSPORTS: [&'static [&'static str]; 4] = [
        &[p::TRANSPORT_UTP, p::TRANSPORT_TCP],
        &[p::TRANSPORT_TCP, p::TRANSPORT_UTP],
        &[p::TRANSPORT_TCP],
        &[p::TRANSPORT_UTP],
    ];

    fn transports_add(&self, widgets: &mut JsonWidgets, s: &Obj) {
        let imp = self.imp();
        let Some(current) = s.get(p::SGET_PREFERRED_TRANSPORTS).and_then(|v| v.as_array()) else {
            imp.transport_label.set_visible(false);
            imp.transport_combo.set_visible(false);
            return;
        };
        let current: Vec<&str> = current.iter().filter_map(|v| v.as_str()).collect();
        let index = Self::TRANSPORTS.iter().position(|t| *t == current.as_slice()).unwrap_or(0);
        imp.transport_combo.set_selected(index as u32);
        let combo = imp.transport_combo.get();
        widgets.add_custom(p::SGET_PREFERRED_TRANSPORTS, move || {
            json!(Self::TRANSPORTS.get(combo.selected() as usize).unwrap_or(&Self::TRANSPORTS[0]))
        });
    }

    /// The days of the week, a bit each in a number
    fn alt_days_add(&self, widgets: &mut JsonWidgets, obj: &Obj, key: &str) {
        let grid = self.imp().alt_days_grid.get();
        let days = get_i64(obj, key);
        let names = if cfg!(feature = "nl_langinfo") { locale_day_names() } else { None };
        for i in 0..7 {
            if let Some(w) = grid.child_at(i, 0).and_downcast::<gtk::CheckButton>() {
                if let Some(names) = &names {
                    w.set_label(Some(&names[i as usize]));
                }
                w.set_active(days & (1 << i) != 0);
            }
        }
        widgets.add_custom(key, move || {
            let mut days = 0i64;
            for i in 0..7 {
                if grid
                    .child_at(i, 0)
                    .and_downcast::<gtk::CheckButton>()
                    .is_some_and(|w| w.is_active())
                {
                    days += 1 << i;
                }
            }
            json!(days)
        });
    }

    fn update_session(&self) {
        let imp = self.imp();
        let encryption = match imp.encryption_combo.selected() {
            0 => p::ENCRYPTION_REQUIRED,
            2 => p::ENCRYPTION_ALLOWED,
            _ => p::ENCRYPTION_PREFERRED,
        };
        let mut req = p::session_set().with(p::SGET_ENCRYPTION, encryption);
        imp.widgets.borrow().save(req.params_mut());

        if let Some(win) = imp.win.upgrade() {
            win.dispatch_session_set(req);
        }
    }
}
