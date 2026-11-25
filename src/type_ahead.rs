//! Type-ahead search, like GtkTreeView's: typing selects a row that matches
//! the text typed so far. The tree views and the sidebar have it.

use std::cell::{Cell, RefCell};

use gtk::{gdk, glib};

/// A pause this long (in microseconds) starts a new search
const PAUSE: i64 = 1_000_000;

/// Whether a key press with state is a shortcut (with Ctrl, Alt, Meta or
/// Super) rather than typing
pub fn is_shortcut(state: gdk::ModifierType) -> bool {
    state.intersects(
        gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::ALT_MASK
            | gdk::ModifierType::META_MASK
            | gdk::ModifierType::SUPER_MASK,
    )
}

/// The text typed without a pause
#[derive(Default)]
pub struct TypeAhead {
    text: RefCell<String>,
    time: Cell<i64>,
}

impl TypeAhead {
    /// The character a key press types, unless it's a control character or
    /// a shortcut
    pub fn typed_char(keyval: gdk::Key, state: gdk::ModifierType) -> Option<char> {
        keyval.to_unicode().filter(|c| !c.is_control() && !is_shortcut(state))
    }

    /// Adds c to the text (a new one after a pause), and returns the text
    /// casefolded, for comparing with casefolded texts
    pub fn push(&self, c: char) -> String {
        let now = glib::monotonic_time();
        if now - self.time.get() > PAUSE {
            self.text.borrow_mut().clear();
        }
        self.time.set(now);
        self.text.borrow_mut().push(c);
        glib::casefold(self.text.borrow().as_str()).to_string()
    }

    /// Whether a search is being typed (its last character less than a
    /// pause ago)
    pub fn is_typing(&self) -> bool {
        !self.text.borrow().is_empty() && glib::monotonic_time() - self.time.get() <= PAUSE
    }
}
