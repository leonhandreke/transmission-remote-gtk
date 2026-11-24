//! The status bar: the connection status on the left, the free space and the
//! speeds (with the limits) on the right, and the alternative speed toggle.

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::client::Client;
use crate::format;
use crate::i18n::{sprintf, tr};
use crate::prefs::{self, Scope};
use crate::protocol::{self as p, Session};
use crate::torrent_model::Stats;
use crate::util;

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/status-bar.ui")]
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
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StatusBar {
        const NAME: &'static str = "TrgStatusBar";
        type Type = super::StatusBar;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for StatusBar {
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl StatusBar {
        /// From left to right
        fn children(&self) -> [gtk::Widget; 4] {
            [
                self.info_lbl.get().upcast(),
                self.free_lbl.get().upcast(),
                self.speed_lbl.get().upcast(),
                self.turtle_image.get().upcast(),
            ]
        }

        /// The minimum and natural sizes of the children, 0 for hidden ones
        fn measure_children(
            &self,
            orientation: gtk::Orientation,
            for_size: i32,
        ) -> [(i32, i32); 4] {
            self.children().map(|c| {
                if !c.is_visible() {
                    return (0, 0);
                }
                let (min, nat, _, _) = c.measure(orientation, for_size);
                (min, nat)
            })
        }
    }

    /* Like a horizontal box, but a GtkBox would shrink all the labels alike,
     * so the speeds ellipsized while the connection status was still shown in
     * full. The minimum width is that of the ellipses, so the status bar
     * doesn't make the window wider. */
    impl WidgetImpl for StatusBar {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let sizes = self.measure_children(orientation, -1);
            let (min, nat) = if orientation == gtk::Orientation::Horizontal {
                sizes.iter().fold((0, 0), |(min, nat), s| (min + s.0, nat + s.1))
            } else {
                sizes.iter().fold((0, 0), |(min, nat), s| (min.max(s.0), nat.max(s.1)))
            };
            (min, nat, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let sizes = self.measure_children(gtk::Orientation::Horizontal, height);
            let mut widths = sizes.map(|s| s.0);
            let mut left = width - widths.iter().sum::<i32>();
            // The space beyond the minimums goes to the turtle, the speeds and
            // the free space first, and the status label gets the rest
            for i in [3, 2, 1] {
                let extra = (sizes[i].1 - sizes[i].0).clamp(0, left.max(0));
                widths[i] += extra;
                left -= extra;
            }
            widths[0] += left.max(0);

            let rtl = self.obj().direction() == gtk::TextDirection::Rtl;
            let mut x = 0;
            for (child, w) in self.children().iter().zip(widths) {
                if child.is_visible() {
                    let cx = if rtl { width - x - w } else { x };
                    child.size_allocate(&gtk::Allocation::new(cx, 0, w, height), -1);
                    x += w;
                }
            }
        }
    }
}

glib::wrapper! {
    pub struct StatusBar(ObjectSubclass<imp::StatusBar>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

#[gtk::template_callbacks]
impl StatusBar {
    #[template_callback]
    fn turtle_toggle(gesture: &gtk::GestureClick, _n_press: i32, _x: f64, _y: f64) {
        let Some(sb) = gesture.widget().and_then(|w| util::ancestor::<StatusBar>(&w)) else {
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
        if let Some(client) = imp.client.borrow().as_ref() {
            client.dispatch_session_set(req);
        }
    }

    /// The full text of a label, where it's ellipsized
    #[template_callback]
    fn ellipsized_tooltip(
        label: &gtk::Label,
        _x: i32,
        _y: i32,
        _keyboard: bool,
        tooltip: &gtk::Tooltip,
    ) -> bool {
        if !label.layout().is_ellipsized() {
            return false;
        }
        tooltip.set_text(Some(&label.text()));
        true
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

    fn set_connected_label(&self, session: &Session, client: &Client) {
        let profile_name = client
            .prefs()
            .get_string(prefs::KEY_PROFILE_NAME, Scope::Connection)
            .unwrap_or_default();
        self.push_connection_msg(&sprintf(
            &tr("Connected: %s :: %s"),
            &[&profile_name, &session.version],
        ));
    }

    pub fn connect(&self, session: &Session, client: &Client) {
        self.set_connected_label(session, client);
        self.imp().speed_lbl.set_text(&tr("Updating torrents..."));
    }

    pub fn session_update(&self, session: &Session) {
        let imp = self.imp();
        let alt = session.alt_speed_enabled;

        if let Some(free) = session.download_dir_free_space.filter(|f| *f >= 0) {
            imp.free_lbl.set_text(&sprintf(&tr("Free space: %s"), &[&format::size(free)]));
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
        let s = &session;
        let (down_limit, up_limit) = if s.alt_speed_enabled {
            (Some(s.alt_speed_down), Some(s.alt_speed_up))
        } else {
            (
                s.speed_limit_down_enabled.then_some(s.speed_limit_down),
                s.speed_limit_up_enabled.then_some(s.speed_limit_up),
            )
        };

        let limit = |l: Option<i64>| match l {
            Some(l) => sprintf(&tr(" (Limit: %s)"), &[&format::speed_kbps(l as f64)]),
            None => String::new(),
        };

        let text = sprintf(
            &tr("Down: %s%s, Up: %s%s"),
            &[
                &format::speed(stats.down_rate_total),
                &limit(down_limit),
                &format::speed(stats.up_rate_total),
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

    /// The main window has the status bar in its template, so this is set later
    pub fn set_client(&self, client: &Client) {
        *self.imp().client.borrow_mut() = Some(client.clone());
    }
}
