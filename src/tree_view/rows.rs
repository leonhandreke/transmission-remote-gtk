//! Finding the rows of widgets and points, for context menus and the
//! keyboard focus

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene};

use super::{TreeView, row_object};

const DATA_LIST_ITEM: &str = "trg-list-item";

/// For row widgets of a list factory (and cells), so their rows can be found
pub fn mark_row_widget(widget: &impl IsA<gtk::Widget>, list_item: &gtk::ListItem) {
    unsafe {
        widget.set_data(DATA_LIST_ITEM, list_item.downgrade());
    }
}

pub(super) fn marked_list_item(widget: &gtk::Widget) -> Option<gtk::ListItem> {
    unsafe {
        widget
            .data::<glib::WeakRef<gtk::ListItem>>(DATA_LIST_ITEM)
            .and_then(|p| p.as_ref().upgrade())
    }
}

/// The list item of the row (or cell) that widget is in, looking up to stop
fn list_item_for(widget: Option<gtk::Widget>, stop: &gtk::Widget) -> Option<gtk::ListItem> {
    let mut widget = widget;
    while let Some(w) = widget {
        if &w == stop {
            break;
        }
        if let Some(li) = marked_list_item(&w) {
            return Some(li);
        }
        let parent = w.parent();
        // the padding of a row (a child of the list view) is outside its
        // row widget or cells
        if parent.as_ref().is_some_and(|p| p.is::<gtk::ListView>()) {
            return find_marked(&w);
        }
        widget = parent;
    }
    None
}

/// The list item of widget or of the first of its descendants that has one
fn find_marked(widget: &gtk::Widget) -> Option<gtk::ListItem> {
    if let Some(li) = marked_list_item(widget) {
        return Some(li);
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        if let Some(li) = find_marked(&c) {
            return Some(li);
        }
        child = c.next_sibling();
    }
    None
}

impl TreeView {
    /// Whether x, y (in the view's coordinates) is in the column headers
    pub fn point_in_header(&self, x: f64, y: f64) -> bool {
        let cv: &gtk::Widget = self.imp().column_view.upcast_ref();
        let mut widget = self.pick(x, y, gtk::PickFlags::DEFAULT);
        while let Some(w) = widget {
            if &w == cv {
                break;
            }
            if w.css_name() == "header" && w.parent().as_ref() == Some(cv) {
                return true;
            }
            widget = w.parent();
        }
        false
    }

    /// The row object of the row at x, y (in the view's coordinates), and
    /// its position
    pub fn item_at(&self, x: f64, y: f64) -> Option<(glib::Object, u32)> {
        // cells may not be targetable, like a text cell that can't be edited
        let widget = self.pick(x, y, gtk::PickFlags::INSENSITIVE | gtk::PickFlags::NON_TARGETABLE);
        let li = list_item_for(widget, self.upcast_ref())?;
        let row = li.item()?;
        Some((row_object(row)?, li.position()))
    }

    /// Whether a context menu asked for at x, y (-1 from the keyboard) is
    /// for the selected rows: it's from the keyboard, or on a row, which is
    /// selected then. The column headers have their own menu.
    pub fn context_menu_for_rows(&self, x: f64, y: f64) -> bool {
        x < 0.0 || (!self.point_in_header(x, y) && self.select_at(x, y))
    }

    /// For a context menu: select the row at x, y unless it's already
    /// selected. Returns whether there was a row.
    pub fn select_at(&self, x: f64, y: f64) -> bool {
        let Some((_, position)) = self.item_at(x, y) else {
            return false;
        };
        let sel = &self.imp().selection;
        if !sel.is_selected(position) {
            sel.select_item(position, true);
        }
        true
    }

    pub(super) fn focus_widget(&self) -> Option<gtk::Widget> {
        let focus = self.root()?.focus()?;
        focus.is_ancestor(self).then_some(focus)
    }

    /// The bounds of the row with the keyboard focus, for a context menu
    /// opened from the keyboard.
    pub fn focus_rect(&self) -> Option<graphene::Rect> {
        let focus = self.focus_widget()?;
        // the focus is on the row widget, or in one of its cells
        if find_marked(&focus).is_none() {
            list_item_for(Some(focus.clone()), self.upcast_ref())?;
        }
        focus.compute_bounds(self)
    }

    /// The list item (of a cell, in a column view) of the row with the
    /// keyboard focus
    pub(super) fn focus_list_item(&self) -> Option<gtk::ListItem> {
        let focus = self.focus_widget()?;
        find_marked(&focus).or_else(|| list_item_for(Some(focus), self.upcast_ref()))
    }

    /// The row (a GtkTreeListRow of a tree) with the keyboard focus
    pub(super) fn focus_row(&self) -> Option<glib::Object> {
        self.focus_list_item()?.item()
    }
}
