//! Type-ahead search, like GtkTreeView's: typing selects the first row
//! whose search column starts with (or else contains) the typed text.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};

use super::value::Val;
use super::{TreeView, row_object};

impl TreeView {
    pub fn set_search_column(&self, property: &'static str) {
        self.imp().search_column.set(Some(property));
    }

    fn search(&self, key: &str) -> bool {
        let imp = self.imp();
        let model = &imp.selection;
        let needle = glib::casefold(key).to_string();
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
            .position(|t| t.as_ref().is_some_and(|t| t.starts_with(&needle)))
            .or_else(|| texts.iter().position(|t| t.as_ref().is_some_and(|t| t.contains(&needle))));

        match found {
            Some(i) => {
                self.select_and_focus(i as u32);
                true
            }
            None => false,
        }
    }

    pub(super) fn other_modifiers(state: gdk::ModifierType) -> bool {
        state.intersects(
            gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::ALT_MASK
                | gdk::ModifierType::META_MASK
                | gdk::ModifierType::SUPER_MASK,
        )
    }

    pub(super) fn key_pressed(&self, keyval: gdk::Key, state: gdk::ModifierType) -> bool {
        let imp = self.imp();
        let Some(c) = keyval.to_unicode() else { return false };
        if imp.search_column.get().is_none() || c.is_control() || Self::other_modifiers(state) {
            return false;
        }
        let now = glib::monotonic_time();
        // a pause starts a new search
        if now - imp.search_time.get() > 1_000_000 {
            imp.search.borrow_mut().clear();
        }
        imp.search_time.set(now);
        imp.search.borrow_mut().push(c);
        let key = imp.search.borrow().clone();
        self.search(&key);
        true
    }
}
