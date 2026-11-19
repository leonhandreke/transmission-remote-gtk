//! The toolbar: a box of buttons with the "toolbar" style class. The buttons
//! activate the main window's actions, so they are made (in)sensitive along
//! with the menu items. It also has the server picker, which shows the
//! connection.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, gio, glib};

use crate::client::ConnectionState;
use crate::i18n::{sprintf, tr};
use crate::menu_bar::file_connect_menu_new;
use crate::prefs::Prefs;
use crate::util;

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-toolbar.ui")]
    pub struct Toolbar {
        #[template_child]
        pub server_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub status_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub status_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub spinner_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub server_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub server_menu: TemplateChild<gio::Menu>,
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
    pub struct Toolbar(ObjectSubclass<imp::Toolbar>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Toolbar {
    pub fn new(prefs: &Prefs) -> Self {
        let tb: Self = glib::Object::new();
        let imp = tb.imp();
        imp.server_menu.prepend_section(None, &file_connect_menu_new(prefs));
        // Under the button from its start, instead of centered, which would
        // stick out of the window
        if let Some(popover) = imp.server_button.popover() {
            popover.set_halign(gtk::Align::Start);
        }
        tb
    }

    /// The server picker shows the profile connected (or connecting) to, or
    /// the one Connect would use, and the state: a green dot while
    /// connected, a spinner while connecting, the dot's outline otherwise.
    /// The state is also in the tooltip and the accessible description, as
    /// the colour alone doesn't tell it to everyone.
    pub fn set_connection(&self, state: ConnectionState, name: &str) {
        let imp = self.imp();
        let (icon, class, description, tooltip) = match state {
            ConnectionState::Connected => (
                "trg-status-connected-symbolic",
                "server-connected",
                tr("Connected"),
                sprintf(&tr("Connected to %s"), &[&name]),
            ),
            ConnectionState::Connecting => (
                "trg-status-disconnected-symbolic",
                "dim-label",
                tr("Connecting"),
                sprintf(&tr("Connecting to %s…"), &[&name]),
            ),
            ConnectionState::Disconnected => (
                "trg-status-disconnected-symbolic",
                "dim-label",
                tr("Not connected"),
                tr("Not connected"),
            ),
        };
        imp.status_icon.set_icon_name(Some(icon));
        imp.status_icon.set_css_classes(&[class]);
        // in the dot's place, which is as wide
        let stack = &imp.status_stack;
        if state != ConnectionState::Connecting {
            stack.set_visible_child_name("icon");
        } else if stack.visible_child_name().as_deref() != Some("spinner") {
            util::restart_spinner(&imp.spinner_box);
            stack.set_visible_child_name("spinner");
        }
        imp.server_label.set_label(name);
        // The name is cut at the window's minimum width, but not to nothing
        imp.server_label.set_width_chars(name.chars().count().min(8) as i32);
        let button = &imp.server_button;
        button.set_tooltip_text(Some(&tooltip));
        button.update_property(&[
            gtk::accessible::Property::Label(name),
            gtk::accessible::Property::Description(&description),
        ]);
    }
}
