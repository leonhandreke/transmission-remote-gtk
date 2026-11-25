//! The cells: the widgets of a column in a row (texts, editable texts,
//! icons and texts, file icons, the wanted check box, priorities and
//! progress bars), set up, bound to the row objects and updated as their
//! properties change

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use super::column::{COLUMN_EDITABLE, ColumnDesc};
use super::editing::{cell_editing_changed, queue_start_editing};
use super::rows::mark_row_widget;
use super::value::{Val, format_value};
use super::{ColType, TreeView, row_object};
use crate::format;
use crate::protocol::TR_PRI_MIXED;

const DATA_CELL: &str = "trg-cell";

/// The widgets of a cell that show its value
enum CellWidget {
    /// A text, formatted by the column's type
    Text(gtk::Label),
    /// The text of an editable column
    Editable(gtk::EditableLabel),
    /// A text and an icon: a named one (IconText) or a file's (FileIconText)
    IconText(gtk::Image, gtk::Label),
    /// The wanted check box
    Wanted(gtk::CheckButton),
    Progress(gtk::ProgressBar),
}

impl CellWidget {
    /// The widgets for a cell of the column desc, and the one that has them
    fn new(desc: &ColumnDesc) -> (Self, gtk::Widget) {
        match desc.ctype {
            ColType::IconText | ColType::FileIconText => {
                let bx = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let image = gtk::Image::new();
                bx.append(&image);
                let label = gtk::Label::new(None);
                label.set_xalign(0.0);
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                label.set_hexpand(true);
                bx.append(&label);
                (CellWidget::IconText(image, label), bx.upcast())
            }
            ColType::Wanted => {
                let check = gtk::CheckButton::new();
                check.set_halign(gtk::Align::Center);
                // the keyboard focus stays on the row, so Enter activates the
                // row rather than toggling the check box
                check.set_focusable(false);
                check.set_focus_on_click(false);
                (CellWidget::Wanted(check.clone()), check.upcast())
            }
            ColType::Prog => {
                let bar = gtk::ProgressBar::new();
                bar.set_show_text(true);
                bar.set_valign(gtk::Align::Center);
                (CellWidget::Progress(bar.clone()), bar.upcast())
            }
            _ if desc.flags & COLUMN_EDITABLE != 0 => {
                let label = gtk::EditableLabel::new("");
                (CellWidget::Editable(label.clone()), label.upcast())
            }
            ctype => {
                let label = gtk::Label::new(None);
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                let xalign = match ctype {
                    ColType::Speed
                    | ColType::Size
                    | ColType::Ratio
                    | ColType::NumGtZero
                    | ColType::NumGtEqZero => 1.0,
                    _ => 0.0,
                };
                label.set_xalign(xalign);
                (CellWidget::Text(label.clone()), label.upcast())
            }
        }
    }
}

/// A cell (the widgets of a column of a row) and what it's bound to
pub(super) struct CellState {
    pub(super) desc: Rc<ColumnDesc>,
    list_item: glib::WeakRef<gtk::ListItem>,
    /// In the file name column of a tree
    expander: Option<gtk::TreeExpander>,
    widget: CellWidget,
    /// The row object
    pub(super) item: RefCell<Option<glib::Object>>,
    /// Of the notify signals of the row object's properties it shows
    handlers: RefCell<Vec<glib::SignalHandlerId>>,
    /// While refresh() sets the check box, which isn't the user toggling it
    updating: Cell<bool>,
    /// The text when the editing started
    pub(super) edit_original: RefCell<Option<String>>,
}

pub(super) fn cell_of(obj: &impl IsA<glib::Object>) -> Option<Rc<CellState>> {
    unsafe { obj.data::<Rc<CellState>>(DATA_CELL).map(|p| p.as_ref().clone()) }
}

/// The icon of a file (by its name), or of a directory (file id -1)
fn file_icon_for(file_id: i64, name: &str) -> gio::Icon {
    if file_id == -1 {
        return gio::ThemedIcon::new("folder").upcast();
    }
    #[cfg(not(windows))]
    {
        let (mime, uncertain) = gio::content_type_guess(Some(name), None::<&[u8]>);
        if !uncertain {
            return gio::content_type_get_icon(&mime);
        }
    }
    gio::ThemedIcon::new("text-x-generic").upcast()
}

impl CellState {
    /// Shows the row object's value
    pub(super) fn refresh(&self) {
        let Some(item) = self.item.borrow().clone() else {
            return;
        };
        let desc = &self.desc;
        let value = Val::of(&item, desc.property);
        self.updating.set(true);

        match &self.widget {
            CellWidget::Progress(bar) => {
                let progress = value.as_f64().clamp(0.0, 100.0);
                bar.set_fraction(progress / 100.0);
                bar.set_text(Some(&format::progress_text(progress)));
            }
            CellWidget::Wanted(check) => {
                let wanted = value.as_i64();
                check.set_inconsistent(wanted == TR_PRI_MIXED);
                check.set_active(wanted == 1);
            }
            CellWidget::IconText(image, label) => {
                let text = value.to_text();
                label.set_text(&text);
                let extra = desc.extra.get().map(|p| Val::of(&item, p));
                if desc.ctype == ColType::IconText {
                    image.set_icon_name(extra.as_ref().and_then(Val::as_str));
                } else {
                    let file_id = extra.as_ref().map_or(0, Val::as_i64);
                    if file_id == -2 {
                        image.clear();
                    } else {
                        image.set_from_gicon(&file_icon_for(file_id, &text));
                    }
                }
            }
            CellWidget::Editable(label) => {
                if !label.is_editing() {
                    label.set_text(format_value(desc.ctype, &value).as_deref().unwrap_or(""));
                }
                // like a GtkTreeView, a click selects the row, and a click
                // on a selected row starts editing
                let selected = self.list_item.upgrade().is_some_and(|li| li.is_selected());
                label.set_can_target(selected);
            }
            CellWidget::Text(label) => {
                label.set_text(format_value(desc.ctype, &value).as_deref().unwrap_or(""));
            }
        }

        self.updating.set(false);
    }

    fn is_editing(&self) -> bool {
        matches!(&self.widget, CellWidget::Editable(label) if label.is_editing())
    }

    /// The user toggled the check box or clicked the priority
    fn activate(&self) {
        let Some(item) = self.item.borrow().clone() else { return };
        let f = self.desc.activate.borrow().clone();
        if let Some(f) = f {
            f(&item);
        }
    }

    fn unbind(&self) {
        if let Some(item) = self.item.take() {
            for h in self.handlers.take() {
                item.disconnect(h);
            }
        }
        if let Some(e) = &self.expander {
            e.set_list_row(None);
        }
    }
}

impl TreeView {
    /// The factory of the cells of the column desc
    pub(super) fn cell_factory(&self, desc: &Rc<ColumnDesc>) -> gtk::SignalListItemFactory {
        let factory = gtk::SignalListItemFactory::new();
        let weak = self.downgrade();
        let d = desc.clone();
        factory.connect_setup(move |_, obj| {
            if let (Some(tv), Some(li)) = (weak.upgrade(), obj.downcast_ref::<gtk::ListItem>()) {
                tv.cell_setup(li, &d);
            }
        });
        let weak = self.downgrade();
        factory.connect_bind(move |_, obj| {
            if let (Some(tv), Some(li)) = (weak.upgrade(), obj.downcast_ref::<gtk::ListItem>()) {
                tv.cell_bind(li);
            }
        });
        factory.connect_unbind(move |_, obj| {
            if let Some(cell) = obj.downcast_ref::<gtk::ListItem>().and_then(cell_of) {
                cell.unbind();
            }
        });
        factory
    }

    fn cell_setup(&self, list_item: &gtk::ListItem, desc: &Rc<ColumnDesc>) {
        let (widget, mut child) = CellWidget::new(desc);

        // the expander of a tree goes in its file name column
        let mut expander = None;
        if desc.ctype == ColType::FileIconText && self.tree_model().is_some() {
            let e = gtk::TreeExpander::new();
            // the keyboard focus stays on the row, as in the other columns:
            // in the expander the row's Enter (opening a directory) didn't
            // reach the row, as its cell takes the key; the view has the
            // expander's keys itself (expand.rs)
            e.set_focusable(false);
            e.set_child(Some(&child));
            child = e.clone().upcast();
            expander = Some(e);
        }

        let cell = Rc::new(CellState {
            desc: desc.clone(),
            list_item: list_item.downgrade(),
            expander,
            widget,
            item: Default::default(),
            handlers: Default::default(),
            updating: Cell::new(false),
            edit_original: Default::default(),
        });

        let weak = Rc::downgrade(&cell);
        match &cell.widget {
            CellWidget::Wanted(check) => {
                check.connect_toggled(move |_| {
                    let Some(cell) = weak.upgrade() else { return };
                    if cell.updating.get() || cell.item.borrow().is_none() {
                        return;
                    }
                    cell.activate();
                    // the model decides the state
                    cell.refresh();
                });
            }
            CellWidget::Editable(label) => {
                let w = weak.clone();
                label.connect_editing_notify(move |label| {
                    if let Some(cell) = w.upgrade() {
                        cell_editing_changed(&cell, label);
                    }
                });
                list_item.connect_selected_notify(move |_| {
                    if let Some(cell) = weak.upgrade()
                        && cell.item.borrow().is_some()
                        && !cell.is_editing()
                    {
                        cell.refresh();
                    }
                });
            }
            CellWidget::Text(label) if desc.ctype == ColType::Prio => {
                let gesture = gtk::GestureClick::new();
                gesture.set_button(gdk::BUTTON_PRIMARY);
                gesture.connect_released(move |gesture, n_press, _, _| {
                    let Some(cell) = weak.upgrade() else { return };
                    if n_press == 1
                        && cell.item.borrow().is_some()
                        && cell.desc.activate.borrow().is_some()
                    {
                        cell.activate();
                        gesture.set_state(gtk::EventSequenceState::Claimed);
                    }
                });
                label.add_controller(gesture);
            }
            _ => {}
        }

        mark_row_widget(&child, list_item);
        unsafe {
            list_item.set_data(DATA_CELL, cell);
        }
        list_item.set_child(Some(&child));
    }

    fn cell_bind(&self, list_item: &gtk::ListItem) {
        let Some(cell) = cell_of(list_item) else { return };
        let Some(row) = list_item.item() else { return };

        if let Some(e) = &cell.expander {
            e.set_list_row(row.downcast_ref::<gtk::TreeListRow>());
        }

        let Some(item) = row_object(row) else { return };
        *cell.item.borrow_mut() = Some(item.clone());

        // (only icon columns have an extra property)
        let properties = std::iter::once(cell.desc.property).chain(cell.desc.extra.get());
        let handlers = properties
            .map(|property| {
                let weak = Rc::downgrade(&cell);
                item.connect_notify_local(Some(property), move |_, _| {
                    if let Some(c) = weak.upgrade() {
                        c.refresh();
                    }
                })
            })
            .collect();
        *cell.handlers.borrow_mut() = handlers;

        cell.refresh();

        if let CellWidget::Editable(label) = &cell.widget
            && self.take_start_edit(&item, &cell.desc)
        {
            queue_start_editing(label);
        }
    }
}
