//! Editing the text cells of editable columns

use std::cell::Cell;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use super::TreeView;
use super::cell::{CellState, cell_of};
use super::column::ColumnDesc;
use super::rows::marked_list_item;

/// Tries for a while, as a cell that was just bound isn't shown yet
pub(super) fn queue_start_editing(label: &gtk::EditableLabel) {
    let tries = Cell::new(0);
    let label = label.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(20), move || {
        if !label.is_mapped() {
            tries.set(tries.get() + 1);
            if tries.get() < 20 && label.parent().is_some() {
                return glib::ControlFlow::Continue;
            }
        } else {
            label.set_can_target(true);
            label.start_editing();
            label.grab_focus();
        }
        glib::ControlFlow::Break
    });
}

impl TreeView {
    /// Whether the cell of item in the column desc should start editing now
    /// that it's shown (it then shouldn't any more)
    pub(super) fn take_start_edit(&self, item: &glib::Object, desc: &Rc<ColumnDesc>) -> bool {
        let mut start_edit = self.imp().start_edit.borrow_mut();
        let matches = start_edit.as_ref().is_some_and(|(i, d)| {
            i.upgrade().as_ref() == Some(item) && d.as_ptr() == Rc::as_ptr(desc)
        });
        if matches {
            start_edit.take();
        }
        matches
    }

    fn find_edit_cell(
        widget: &gtk::Widget,
        item: &glib::Object,
        desc: &Rc<ColumnDesc>,
    ) -> Option<gtk::EditableLabel> {
        if let Some(label) = widget.downcast_ref::<gtk::EditableLabel>() {
            let cell = marked_list_item(widget).and_then(|li| cell_of(&li));
            return cell
                .filter(|c| Rc::ptr_eq(&c.desc, desc) && c.item.borrow().as_ref() == Some(item))
                .map(|_| label.clone());
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            if let Some(found) = Self::find_edit_cell(&c, item, desc) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }

    /// Starts editing the cell of item in the editable column desc, now if
    /// it's shown, or once it's shown.
    pub fn start_editing(&self, item: &impl IsA<glib::Object>, desc: &Rc<ColumnDesc>) {
        let item = item.upcast_ref();
        match Self::find_edit_cell(self.imp().column_view.upcast_ref(), item, desc) {
            Some(label) => queue_start_editing(&label),
            None => {
                *self.imp().start_edit.borrow_mut() = Some((item.downgrade(), Rc::downgrade(desc)));
                if let Some(position) = self.find_position(item) {
                    self.scroll_to(position, gtk::ListScrollFlags::NONE);
                }
            }
        }
    }

    fn stop_editing_in(widget: &gtk::Widget) {
        if let Some(label) = widget.downcast_ref::<gtk::EditableLabel>() {
            if label.is_editing() {
                label.stop_editing(true);
            }
            return;
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            Self::stop_editing_in(&c);
            child = c.next_sibling();
        }
    }

    /// Finishes the editing of any cell, keeping its text. A cell being
    /// edited only does that itself a moment after it loses the focus.
    pub fn stop_editing(&self) {
        Self::stop_editing_in(self.imp().column_view.upcast_ref());
    }
}

pub(super) fn cell_editing_changed(cell: &Rc<CellState>, label: &gtk::EditableLabel) {
    let Some(item) = cell.item.borrow().clone() else { return };
    let edit = cell.desc.edit.borrow().clone();

    if label.is_editing() {
        *cell.edit_original.borrow_mut() = Some(label.text().to_string());
        if let Some(edit) = edit {
            edit(&item, false, None);
        }
    } else {
        let text = label.text().to_string();
        let changed = Some(&text) != cell.edit_original.borrow().as_ref();
        if let Some(edit) = edit {
            edit(&item, true, changed.then_some(text.as_str()));
        }
        cell.edit_original.take();
        if cell.item.borrow().as_ref() == Some(&item) {
            cell.refresh();
        }
    }
}
