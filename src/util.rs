//! URL checks and GTK helpers: error dialogs, context menus and popover
//! menus.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use crate::client::Error;
use crate::i18n::tr;

/* URLs */

pub fn is_magnet(s: &str) -> bool {
    s.starts_with("magnet:")
}

pub fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// The host of a tracker's URL, without "www.", "tracker." and the like,
/// which is what tells the trackers apart. Like the regular expression
/// ^[^:/?#]+:?//(?:www\.|torrent\.|torrents\.|tracker\.|\d+\.)?([^/?#:]*)
pub fn tracker_host(url: &str) -> Option<&str> {
    let is_delimiter = |c| matches!(c, ':' | '/' | '?' | '#');
    let scheme_end = url.find(is_delimiter).filter(|&i| i > 0)?;
    let rest = &url[scheme_end..];
    let mut rest = rest.strip_prefix(':').unwrap_or(rest).strip_prefix("//")?;

    let prefix = ["www.", "torrent.", "torrents.", "tracker."]
        .iter()
        .find_map(|p| rest.strip_prefix(p))
        .or_else(|| {
            let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            rest[digits..].strip_prefix('.').filter(|_| digits > 0)
        });
    if let Some(r) = prefix {
        rest = r;
    }

    let end = rest.find(is_delimiter).unwrap_or(rest.len());
    Some(&rest[..end])
}

pub fn rm_trailing_slashes(s: &str) -> &str {
    s.trim_end_matches('/')
}

/* GTK */

/// The widget of type T that widget is in (the template object of a
/// callback of a template, as the callbacks get the emitter)
pub fn ancestor<T: IsA<gtk::Widget>>(widget: &impl IsA<gtk::Widget>) -> Option<T> {
    let widget = widget.as_ref();
    if let Some(t) = widget.downcast_ref::<T>() {
        return Some(t.clone());
    }
    widget.ancestor(T::static_type()).and_downcast::<T>()
}

/// Whether we're running on GTK's macOS backend, so should follow macOS
/// conventions (native menu bar, Command key shortcuts, ...).
pub fn is_macos() -> bool {
    cfg!(target_os = "macos")
        && gdk::Display::default().is_some_and(|d| d.type_().name() == "GdkMacosDisplay")
}

/// The primary shortcut modifier: Command on macOS, Control elsewhere
pub fn primary_modifier() -> gdk::ModifierType {
    if is_macos() { gdk::ModifierType::META_MASK } else { gdk::ModifierType::CONTROL_MASK }
}

/// Shows an error message, calls done (if any) once it's dismissed.
pub fn error_dialog_full(
    parent: Option<&impl IsA<gtk::Window>>,
    msg: &str,
    done: Option<Box<dyn FnOnce()>>,
) {
    let dialog = gtk::AlertDialog::builder().message(tr("Error")).detail(msg).modal(true).build();
    dialog.choose(parent, gio::Cancellable::NONE, move |_| {
        if let Some(done) = done {
            done();
        }
    });
}

pub fn error_dialog(parent: Option<&impl IsA<gtk::Window>>, msg: &str) {
    error_dialog_full(parent, msg, None);
}

pub fn client_error_dialog(parent: Option<&impl IsA<gtk::Window>>, error: &Error) {
    error_dialog(parent, &error.to_string());
}

/// Context menus: a click gesture for the secondary button and a shortcut for
/// the keyboard (Shift+F10 and the Menu key). x and y are -1 when the menu
/// was requested from the keyboard. func returns whether it showed a menu.
pub fn add_context_menu<W: IsA<gtk::Widget>>(
    widget: &W,
    func: impl Fn(&gtk::Widget, f64, f64) -> bool + 'static,
) {
    let func = Rc::new(func);

    let gesture = gtk::GestureClick::new();
    gesture.set_button(gdk::BUTTON_SECONDARY);
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    let f = func.clone();
    gesture.connect_pressed(move |gesture, n_press, x, y| {
        if n_press != 1 {
            return;
        }
        if let Some(widget) = gesture.widget()
            && f(&widget, x, y)
        {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    widget.add_controller(gesture);

    let shortcuts = gtk::ShortcutController::new();
    let f = func.clone();
    shortcuts.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("<Shift>F10|Menu"),
        Some(gtk::CallbackAction::new(move |widget, _| {
            if f(widget, -1.0, -1.0) { glib::Propagation::Stop } else { glib::Propagation::Proceed }
        })),
    ));
    // capture, so the widget's own key handling can't swallow it
    shortcuts.set_propagation_phase(gtk::PropagationPhase::Capture);
    widget.add_controller(shortcuts);
}

/// Pops up a context menu for model on parent, at x/y (-1 for the whole
/// widget). actions (if any) are inserted with prefix for the items.
pub fn popover_menu_popup(
    parent: &impl IsA<gtk::Widget>,
    model: &impl IsA<gio::MenuModel>,
    mut x: f64,
    mut y: f64,
    prefix: &str,
    actions: Option<&impl IsA<gio::ActionGroup>>,
) {
    let parent = parent.as_ref();
    // nested submenus, like GTK 3 menus, rather than sliding ones
    let popover = gtk::PopoverMenu::from_model_full(model, gtk::PopoverMenuFlags::NESTED);

    if let Some(actions) = actions {
        popover.insert_action_group(prefix, Some(actions));
    }

    // A tree view presents its popovers. From the keyboard, point at the row
    // with the focus if there is one.
    if (x < 0.0 || y < 0.0)
        && let Some(tv) = parent.downcast_ref::<crate::tree_view::TreeView>()
        && let Some(bounds) = tv.focus_rect()
    {
        x = bounds.x() as f64 + 20.0;
        y = bounds.y() as f64 + bounds.height() as f64 / 2.0;
    }

    popover.set_parent(parent);
    popover.set_has_arrow(false);
    popover.set_halign(gtk::Align::Start);

    if x >= 0.0 && y >= 0.0 {
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    }

    popover.connect_closed(|popover| {
        // Defer, the activated item's action is dispatched after "closed".
        let popover = popover.clone();
        glib::idle_add_local_once(move || {
            if popover.parent().is_some() {
                popover.unparent();
            }
        });
    });
    popover.popup();

    // Most widgets don't present popovers they don't know about in their
    // size_allocate, so the menu would keep the size it had before its items
    // were fully set up (eg. accelerator labels). Re-present it for the
    // first few frames.
    let frames = Cell::new(0);
    popover.add_tick_callback(move |popover, _| {
        if popover.parent().is_some() && popover.is_visible() {
            popover.present();
        }
        frames.set(frames.get() + 1);
        if frames.get() < 3 { glib::ControlFlow::Continue } else { glib::ControlFlow::Break }
    });
}

/// Puts a new spinner, spinning, first in a box, in place of the one
/// that's there. GTK 4.24's spinner pauses its animation when it's unmapped
/// (in a hidden stack page, say) and doesn't play it again when it's mapped,
/// so it would show a frozen frame, or nothing, the next time.
pub fn restart_spinner(container: &gtk::Box) {
    if let Some(old) = container.first_child().and_downcast::<gtk::Spinner>() {
        container.remove(&old);
    }
    container.prepend(&gtk::Spinner::builder().spinning(true).build());
}

/// A scrolled window with a frame around child
pub fn scrolled_window(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .has_frame(true)
        .child(child)
        .build()
}

/// The items (of type T) of a list model
pub fn objects<T: IsA<glib::Object>>(
    model: &impl IsA<gio::ListModel>,
) -> impl Iterator<Item = T> + '_ {
    (0..model.n_items()).filter_map(move |i| model.item(i).and_downcast::<T>())
}

/// Removes the items (of type T) for which stale is true, in runs from the
/// end, so it's linear in the number of items. Returns how many were removed.
pub fn remove_where<T: IsA<glib::Object>>(
    store: &gio::ListStore,
    mut stale: impl FnMut(&T) -> bool,
) -> u32 {
    let mut removed = 0;
    let mut i = store.n_items();
    while i > 0 {
        let end = i;
        while i > 0 {
            let item = store.item(i - 1).and_downcast::<T>().unwrap();
            if !stale(&item) {
                break;
            }
            i -= 1;
        }
        if i < end {
            store.splice(i, end - i, &[] as &[glib::Object]);
            removed += end - i;
        } else {
            i -= 1;
        }
    }
    removed
}

/// Sets a property of obj, notifying only if its value changes (setting a
/// property notifies even if it doesn't), so the cells showing it don't
/// refresh for nothing
pub fn set_changed<V>(obj: &impl IsA<glib::Object>, name: &str, value: V)
where
    V: Into<glib::Value> + PartialEq + for<'a> glib::value::FromValue<'a> + 'static,
{
    if obj.property::<V>(name) != value {
        obj.set_property(name, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_hosts() {
        assert_eq!(tracker_host("udp://tracker.example.org:6969/announce"), Some("example.org"));
        assert_eq!(tracker_host("https://www.example.org/announce?x"), Some("example.org"));
        assert_eq!(tracker_host("http://torrents.example.org"), Some("example.org"));
        assert_eq!(tracker_host("http://9.example.org/"), Some("example.org"));
        assert_eq!(tracker_host("http://torrentsite.org/"), Some("torrentsite.org"));
        assert_eq!(tracker_host("http:example.org"), None);
        assert_eq!(tracker_host("//example.org"), None);
        assert_eq!(tracker_host("example.org:443"), None);
    }
}
