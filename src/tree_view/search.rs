//! Type-ahead search, like GtkTreeView's: typing selects the first row
//! whose search column starts with (or else contains) the typed text.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};

use super::value::Val;
use super::{TreeView, row_object};
use crate::type_ahead::TypeAhead;

impl TreeView {
    pub fn set_search_column(&self, property: &'static str) {
        self.imp().search_column.set(Some(property));
    }

    /// Selects the first row whose text starts with needle (casefolded),
    /// or else contains it
    fn search(&self, needle: &str) -> bool {
        let imp = self.imp();
        let model = &imp.selection;
        let Some(property) = imp.search_column.get() else { return false };
        let texts: Vec<Option<String>> = (0..model.n_items())
            .map(|i| {
                model
                    .item(i)
                    .and_then(row_object)
                    .and_then(|obj| Val::of(&obj, property).to_text())
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

    pub(super) fn key_pressed(&self, keyval: gdk::Key, state: gdk::ModifierType) -> bool {
        let imp = self.imp();
        let Some(c) = TypeAhead::typed_char(keyval, state) else { return false };
        if imp.search_column.get().is_none() {
            return false;
        }
        let needle = imp.type_ahead.push(c);
        self.search(&needle);
        true
    }
}
