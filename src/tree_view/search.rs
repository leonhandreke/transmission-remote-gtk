//! Type-ahead search, like GtkTreeView's: typing selects the first row
//! whose search column starts with (or else contains) the typed text, and
//! moves the keyboard focus there.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};

use super::value::Val;
use super::{TreeView, row_object};
use crate::type_ahead::TypeAhead;

impl TreeView {
    /// The view's keys, in the capture phase: before the list's own, which
    /// take Space (to select the row with the focus) and Shift+arrows (to
    /// extend the selection). Expanding and collapsing rows first, then the
    /// search; while a search is being typed, every key it takes (a space
    /// too) is search text.
    pub(super) fn setup_keys(&self) {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, keyval, _, state| match weak.upgrade() {
            Some(tv)
                if !tv.focus_takes_keys()
                    && (tv.expand_key_pressed(keyval, state) || tv.key_pressed(keyval, state)) =>
            {
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        self.add_controller(keys);
    }

    /// Whether the focus is in a text field (a cell being edited) or a
    /// popover (a context menu) of the view, which get the keys themselves
    fn focus_takes_keys(&self) -> bool {
        self.focus_widget().is_some_and(|f| {
            f.is::<gtk::Text>()
                || f.ancestor(gtk::Popover::static_type()).is_some_and(|p| p.is_ancestor(self))
        })
    }

    /// Typing searches the property of the rows
    pub fn set_search_column(&self, property: &'static str) {
        self.imp().search_column.set(Some(property));
    }

    /// Selects the first row whose text starts with needle (casefolded),
    /// or else contains it, and moves the focus there
    fn search(&self, needle: &str) -> bool {
        let imp = self.imp();
        let model = &imp.selection;
        let Some(property) = imp.search_column.get() else { return false };
        let texts: Vec<Option<String>> = (0..model.n_items())
            .map(|i| {
                model
                    .item(i)
                    .and_then(row_object)
                    .map(|obj| Val::of(&obj, property).to_text())
                    .map(|t| glib::casefold(&t).to_string())
            })
            .collect();

        let found = texts
            .iter()
            .position(|t| t.as_ref().is_some_and(|t| t.starts_with(needle)))
            .or_else(|| texts.iter().position(|t| t.as_ref().is_some_and(|t| t.contains(needle))));

        match found {
            Some(i) => {
                self.select_and_focus(i as u32);
                true
            }
            None => false,
        }
    }

    fn key_pressed(&self, keyval: gdk::Key, state: gdk::ModifierType) -> bool {
        let imp = self.imp();
        let Some(c) = TypeAhead::typed_char(keyval, state) else { return false };
        // a space starts no search: it selects the row with the focus
        if imp.search_column.get().is_none() || (c.is_whitespace() && !imp.type_ahead.is_typing()) {
            return false;
        }
        let needle = imp.type_ahead.push(c);
        self.search(&needle);
        true
    }
}
