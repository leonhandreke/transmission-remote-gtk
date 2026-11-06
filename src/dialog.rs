//! TrgDialog: a window with a content area and a row of buttons, which
//! respond with a response ID like GtkDialog (deprecated in GTK 4.10) did.
//!
//! In a template, children go into the content area, and children of type
//! "action" ([action] in Blueprint) into the row of buttons, which respond
//! through the dialog.* actions: "dialog.accept" responds with ACCEPT, and so
//! on. They're separate actions, so a response can be disabled (and its
//! button made insensitive) with action_set_enabled().

use std::cell::{Cell, RefCell};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};

pub const RESPONSE_ACCEPT: i32 = -3;
pub const RESPONSE_DELETE_EVENT: i32 = -4;
pub const RESPONSE_OK: i32 = -5;
pub const RESPONSE_CANCEL: i32 = -6;
pub const RESPONSE_CLOSE: i32 = -7;
pub const RESPONSE_YES: i32 = -8;
pub const RESPONSE_NO: i32 = -9;
pub const RESPONSE_APPLY: i32 = -10;

const RESPONSE_ACTIONS: &[(&str, i32)] = &[
    ("accept", RESPONSE_ACCEPT),
    ("cancel", RESPONSE_CANCEL),
    ("ok", RESPONSE_OK),
    ("close", RESPONSE_CLOSE),
    ("apply", RESPONSE_APPLY),
    ("yes", RESPONSE_YES),
    ("no", RESPONSE_NO),
];

const GUI_PAD: i32 = 6;
const GUI_PAD_BIG: i32 = 12;

mod imp {
    use super::*;
    use std::sync::OnceLock;

    pub struct Dialog {
        pub content_area: gtk::Box,
        pub action_area: gtk::Box,
        pub destroyed: Cell<bool>,
        pub responded: RefCell<Option<i32>>,
    }

    impl Default for Dialog {
        fn default() -> Self {
            Self {
                content_area: gtk::Box::new(gtk::Orientation::Vertical, 0),
                action_area: gtk::Box::new(gtk::Orientation::Horizontal, GUI_PAD),
                destroyed: Cell::new(false),
                responded: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Dialog {
        const NAME: &'static str = "TrgDialog";
        type Type = super::Dialog;
        type ParentType = gtk::Window;
        type Interfaces = (gtk::Buildable,);

        fn class_init(klass: &mut Self::Class) {
            klass.add_binding_action(gdk::Key::Escape, gdk::ModifierType::empty(), "window.close");
            for (nick, response) in RESPONSE_ACTIONS {
                let response = *response;
                klass.install_action(&format!("dialog.{nick}"), None, move |dialog, _, _| {
                    use super::TrgDialogExt;
                    dialog.response(response);
                });
            }
        }
    }

    impl ObjectImpl for Dialog {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("response")
                        .param_types([i32::static_type()])
                        .build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }

        fn dispose(&self) {
            self.destroyed.set(true);
        }
    }

    impl WidgetImpl for Dialog {}

    impl WindowImpl for Dialog {
        /// Closing the window is a DELETE_EVENT response, like GtkDialog. The
        /// handlers usually destroy the window themselves.
        fn close_request(&self) -> glib::Propagation {
            let obj = self.obj().clone();
            super::TrgDialogExt::response(&obj, RESPONSE_DELETE_EVENT);
            if self.destroyed.get() {
                return glib::Propagation::Stop;
            }
            self.parent_close_request()
        }
    }

    impl BuildableImpl for Dialog {
        fn add_child(&self, builder: &gtk::Builder, child: &glib::Object, type_: Option<&str>) {
            if type_ == Some("action") {
                if let Some(w) = child.downcast_ref::<gtk::Widget>() {
                    self.action_area.append(w);
                    self.action_area.set_visible(true);
                }
            } else if type_.is_none()
                && child.is::<gtk::Widget>()
                && self.content_area.parent().is_some()
            {
                self.content_area.append(child.downcast_ref::<gtk::Widget>().unwrap());
            } else {
                self.parent_add_child(builder, child, type_);
            }
        }
    }
}

glib::wrapper! {
    pub struct Dialog(ObjectSubclass<imp::Dialog>)
        @extends gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root,
            gtk::ShortcutManager;
}

pub trait DialogImpl: WindowImpl {}

unsafe impl<T: DialogImpl> IsSubclassable<T> for Dialog {
    fn instance_init(instance: &mut glib::subclass::InitializingObject<T>) {
        Self::parent_instance_init::<T>(instance);
        // the boxes have to be there before the template's children are added
        let dialog = unsafe { instance.as_ref().unsafe_cast_ref::<Dialog>() };
        dialog.setup();
    }
}

impl Dialog {
    fn setup(&self) {
        let imp = self.imp();
        if imp.content_area.parent().is_some() {
            return;
        }
        let bx = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let content = &imp.content_area;
        content.set_vexpand(true);
        content.set_margin_start(GUI_PAD);
        content.set_margin_end(GUI_PAD);
        content.set_margin_top(GUI_PAD);
        content.set_margin_bottom(GUI_PAD);
        bx.append(content);

        let actions = &imp.action_area;
        actions.set_halign(gtk::Align::End);
        actions.set_margin_start(GUI_PAD_BIG);
        actions.set_margin_end(GUI_PAD_BIG);
        actions.set_margin_top(GUI_PAD);
        actions.set_margin_bottom(GUI_PAD_BIG);
        actions.set_visible(false);
        bx.append(actions);

        self.set_child(Some(&bx));
        self.add_css_class("dialog");
    }
}

/// The methods of a dialog, for its subclasses
pub trait TrgDialogExt: IsA<Dialog> + 'static {
    fn response(&self, response: i32) {
        self.emit_by_name::<()>("response", &[&response]);
    }

    /// Calls f with the dialog and the response
    fn on_response<F: Fn(&Self, i32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("response", false, move |args| {
            let d = args[0].get::<Dialog>().unwrap();
            f(d.downcast_ref::<Self>().unwrap(), args[1].get::<i32>().unwrap());
            None
        })
    }
}

impl<T: IsA<Dialog>> TrgDialogExt for T {}

/// The widget of type T that widget is in (the template object of a
/// callback of a template, as the callbacks get the emitter)
pub fn ancestor<T: IsA<gtk::Widget>>(widget: &impl IsA<gtk::Widget>) -> Option<T> {
    let widget = widget.as_ref();
    if let Some(t) = widget.downcast_ref::<T>() {
        return Some(t.clone());
    }
    widget.ancestor(T::static_type()).and_downcast::<T>()
}
