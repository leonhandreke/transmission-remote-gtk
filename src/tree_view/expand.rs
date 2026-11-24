//! Expanding and collapsing the rows of a tree: all of them, and with the
//! keys of GtkTreeView

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};

use super::TreeView;

impl TreeView {
    fn tree_model(&self) -> Option<gtk::TreeListModel> {
        self.model().and_downcast::<gtk::TreeListModel>()
    }

    fn set_all_expanded(&self, expanded: bool) {
        let Some(model) = self.tree_model() else { return };
        // expanding inserts the children after the row, so they're seen too
        let mut i = 0;
        while i < model.n_items() {
            if let Some(row) = model.row(i) {
                row.set_expanded(expanded);
            }
            i += 1;
        }
    }

    pub fn expand_all(&self) {
        self.set_all_expanded(true);
    }

    pub fn collapse_all(&self) {
        self.set_all_expanded(false);
    }

    fn expand_recursive(row: &gtk::TreeListRow) {
        row.set_expanded(true);
        let n = row.children().map(|c| c.n_items()).unwrap_or(0);
        for i in 0..n {
            if let Some(child) = row.child_row(i) {
                Self::expand_recursive(&child);
            }
        }
    }

    /// The keys of GtkTreeView (and GtkTreeExpander) to expand or collapse
    /// the row with the focus: + and Shift+Right expand, * expands all below,
    /// - and / and Shift+Left collapse.
    fn expand_key(&self, keyval: gdk::Key, state: gdk::ModifierType) -> bool {
        let Some(row) = self.focus_row().and_downcast::<gtk::TreeListRow>() else {
            return false;
        };
        if !row.is_expandable() {
            return false;
        }
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        use gdk::Key;
        match keyval {
            Key::plus | Key::KP_Add => {
                row.set_expanded(true);
                true
            }
            Key::asterisk | Key::KP_Multiply => {
                Self::expand_recursive(&row);
                true
            }
            Key::minus | Key::KP_Subtract | Key::slash | Key::KP_Divide => {
                row.set_expanded(false);
                true
            }
            Key::Right | Key::KP_Right => {
                if shift {
                    row.set_expanded(true);
                }
                shift
            }
            Key::Left | Key::KP_Left => {
                if shift {
                    row.set_expanded(false);
                }
                shift
            }
            _ => false,
        }
    }

    /// Before the list's own keys (Shift+arrows extend the selection)
    pub(super) fn expand_key_pressed(&self, keyval: gdk::Key, state: gdk::ModifierType) -> bool {
        let imp = self.imp();
        let searching = !imp.search.borrow().is_empty()
            && glib::monotonic_time() - imp.search_time.get() <= 1_000_000;
        // unless they're part of a search
        if searching || self.tree_model().is_none() || Self::other_modifiers(state) {
            return false;
        }
        self.expand_key(keyval, state)
    }
}
