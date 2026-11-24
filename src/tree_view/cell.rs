//! The cells: the widgets of a column in a row (texts, editable texts,
//! icons and texts, file icons, the wanted check box, priorities and
//! progress bars), set up, bound to the row objects and updated as their
//! properties change

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib};

use super::column::{COLUMN_EDITABLE, ColumnDesc};
use super::editing::{cell_editing_changed, queue_start_editing};
use super::rows::mark_row_widget;
use super::value::{Val, format_value};
use super::{ColType, TreeView, row_object};
use crate::format;
use crate::protocol::TR_PRI_MIXED;

const DATA_CELL: &str = "trg-cell";

/// A cell (the widgets of a column of a row) and what it's bound to
pub(super) struct CellState {
    pub(super) desc: Rc<ColumnDesc>,
    pub(super) tv: glib::WeakRef<TreeView>,
    pub(super) list_item: glib::WeakRef<gtk::ListItem>,
    pub(super) expander: Option<gtk::TreeExpander>,
    pub(super) image: Option<gtk::Image>,
    pub(super) widget: gtk::Widget,
    /// The row object
    pub(super) item: RefCell<Option<glib::Object>>,
    pub(super) handlers: RefCell<Vec<glib::SignalHandlerId>>,
    pub(super) updating: Cell<bool>,
    pub(super) edit_original: RefCell<Option<String>>,
}

pub(super) fn cell_of(obj: &impl IsA<glib::Object>) -> Option<Rc<CellState>> {
    unsafe { obj.data::<Rc<CellState>>(DATA_CELL).map(|p| p.as_ref().clone()) }
}

/// The icon of a file (by its name), or of a directory (file id -1)
fn file_icon_for(file_id: i64, name: Option<&str>) -> gio::Icon {
    if file_id == -1 {
        return gio::ThemedIcon::new("folder").upcast();
    }
    #[cfg(not(windows))]
    if let Some(name) = name {
        let (mime, uncertain) = gio::content_type_guess(Some(name), None::<&[u8]>);
        if !uncertain {
            return gio::content_type_get_icon(&mime);
        }
    }
    gio::ThemedIcon::new("text-x-generic").upcast()
}

impl CellState {
    pub(super) fn refresh(&self) {
        let Some(item) = self.item.borrow().clone() else {
            return;
        };
        let desc = &self.desc;
        let value = Val::of(&item, desc.property);
        let extra = desc.extra.get().map(|p| Val::of(&item, p));
        self.updating.set(true);

        match desc.ctype {
            ColType::Prog => {
                let progress = value.as_f64().clamp(0.0, 100.0);
                let bar = self.widget.downcast_ref::<gtk::ProgressBar>().unwrap();
                bar.set_fraction(progress / 100.0);
                bar.set_text(Some(&format::progress_text(progress)));
            }
            ColType::Wanted => {
                let wanted = value.as_i64();
                let check = self.widget.downcast_ref::<gtk::CheckButton>().unwrap();
                check.set_inconsistent(wanted == TR_PRI_MIXED);
                check.set_active(wanted == 1);
            }
            ColType::IconText | ColType::FileIconText => {
                let text = value.to_text();
                self.widget
                    .downcast_ref::<gtk::Label>()
                    .unwrap()
                    .set_text(text.as_deref().unwrap_or(""));
                let image = self.image.as_ref().unwrap();
                if desc.ctype == ColType::IconText {
                    image.set_icon_name(extra.as_ref().and_then(Val::as_str));
                } else {
                    let file_id = extra.as_ref().map_or(0, Val::as_i64);
                    if file_id == -2 {
                        image.clear();
                    } else {
                        image.set_from_gicon(&file_icon_for(file_id, text.as_deref()));
                    }
                }
            }
            _ => {
                let text = format_value(desc, &value);
                if let Some(label) = self.widget.downcast_ref::<gtk::EditableLabel>() {
                    if !label.is_editing() {
                        label.set_text(text.as_deref().unwrap_or(""));
                    }
                    // like a GtkTreeView, a click selects the row, and a click
                    // on a selected row starts editing
                    let selected = self.list_item.upgrade().is_some_and(|li| li.is_selected());
                    label.set_can_target(desc.editable.get() && selected);
                    label.set_editable(desc.editable.get());
                } else if let Some(label) = self.widget.downcast_ref::<gtk::Label>() {
                    label.set_text(text.as_deref().unwrap_or(""));
                }
            }
        }

        self.updating.set(false);
    }

    pub(super) fn activate(&self) {
        let (Some(tv), Some(item)) = (self.tv.upgrade(), self.item.borrow().clone()) else {
            return;
        };
        let f = self.desc.activate.borrow().clone();
        if let Some(f) = f {
            f(&tv, &item);
        }
    }
}

impl TreeView {
    pub(super) fn cell_setup(&self, list_item: &gtk::ListItem, desc: &Rc<ColumnDesc>) {
        let mut image = None;
        let widget: gtk::Widget;
        let mut child: gtk::Widget;

        match desc.ctype {
            ColType::IconText | ColType::FileIconText => {
                let bx = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let img = gtk::Image::new();
                bx.append(&img);
                let label = gtk::Label::new(None);
                label.set_xalign(0.0);
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                label.set_hexpand(true);
                bx.append(&label);
                image = Some(img);
                widget = label.upcast();
                child = bx.upcast();
            }
            ColType::Wanted => {
                let check = gtk::CheckButton::new();
                check.set_halign(gtk::Align::Center);
                // the keyboard focus stays on the row, so Enter activates the
                // row rather than toggling the check box
                check.set_focusable(false);
                check.set_focus_on_click(false);
                widget = check.clone().upcast();
                child = check.upcast();
            }
            ColType::Prog => {
                let bar = gtk::ProgressBar::new();
                bar.set_show_text(true);
                bar.set_valign(gtk::Align::Center);
                widget = bar.clone().upcast();
                child = bar.upcast();
            }
            _ => {
                if desc.flags.get() & COLUMN_EDITABLE != 0 {
                    let label = gtk::EditableLabel::new("");
                    widget = label.clone().upcast();
                    child = label.upcast();
                } else {
                    let label = gtk::Label::new(None);
                    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    let xalign = match desc.ctype {
                        ColType::Speed
                        | ColType::Size
                        | ColType::Ratio
                        | ColType::NumGtZero
                        | ColType::NumGtEqZero => 1.0,
                        _ => 0.0,
                    };
                    label.set_xalign(xalign);
                    widget = label.clone().upcast();
                    child = label.upcast();
                }
            }
        }

        // the expander of a tree goes in its file name column
        let mut expander = None;
        let is_tree =
            self.imp().model.borrow().as_ref().is_some_and(|m| m.is::<gtk::TreeListModel>());
        if desc.ctype == ColType::FileIconText && is_tree {
            let e = gtk::TreeExpander::new();
            e.set_child(Some(&child));
            child = e.clone().upcast();
            expander = Some(e);
        }

        let cell = Rc::new(CellState {
            desc: desc.clone(),
            tv: self.downgrade(),
            list_item: list_item.downgrade(),
            expander,
            image,
            widget: widget.clone(),
            item: Default::default(),
            handlers: Default::default(),
            updating: Cell::new(false),
            edit_original: Default::default(),
        });

        let weak = Rc::downgrade(&cell);
        if let Some(check) = widget.downcast_ref::<gtk::CheckButton>() {
            let weak = weak.clone();
            check.connect_toggled(move |_| {
                let Some(cell) = weak.upgrade() else { return };
                if cell.updating.get() || cell.item.borrow().is_none() {
                    return;
                }
                cell.activate();
                // the model decides the state
                cell.refresh();
            });
        } else if let Some(label) = widget.downcast_ref::<gtk::EditableLabel>() {
            let w = weak.clone();
            label.connect_editing_notify(move |label| {
                if let Some(cell) = w.upgrade() {
                    cell_editing_changed(&cell, label);
                }
            });
            let w = weak.clone();
            list_item.connect_selected_notify(move |_| {
                if let Some(cell) = w.upgrade() {
                    let editing =
                        cell.widget.downcast_ref::<gtk::EditableLabel>().unwrap().is_editing();
                    if cell.item.borrow().is_some() && !editing {
                        cell.refresh();
                    }
                }
            });
        }

        if desc.ctype == ColType::Prio {
            let gesture = gtk::GestureClick::new();
            gesture.set_button(gdk::BUTTON_PRIMARY);
            let w = weak.clone();
            gesture.connect_released(move |gesture, n_press, _, _| {
                let Some(cell) = w.upgrade() else { return };
                if n_press == 1
                    && cell.item.borrow().is_some()
                    && cell.desc.activate.borrow().is_some()
                {
                    cell.activate();
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }
            });
            widget.add_controller(gesture);
        }

        mark_row_widget(&child, list_item);
        unsafe {
            list_item.set_data(DATA_CELL, cell);
        }
        list_item.set_child(Some(&child));
    }

    pub(super) fn cell_bind(&self, list_item: &gtk::ListItem) {
        let Some(cell) = cell_of(list_item) else { return };
        let Some(row) = list_item.item() else { return };

        if let Some(e) = &cell.expander {
            e.set_list_row(row.downcast_ref::<gtk::TreeListRow>());
        }

        let Some(item) = row_object(row) else { return };
        *cell.item.borrow_mut() = Some(item.clone());

        let mut properties = vec![cell.desc.property];
        if matches!(cell.desc.ctype, ColType::IconText | ColType::FileIconText) {
            properties.extend(cell.desc.extra.get());
        }
        let handlers = properties
            .into_iter()
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

        if let Some(label) = cell.widget.downcast_ref::<gtk::EditableLabel>()
            && self.take_start_edit(&item, &cell.desc)
        {
            queue_start_editing(label);
        }
    }

    pub(super) fn cell_unbind(&self, list_item: &gtk::ListItem) {
        let Some(cell) = cell_of(list_item) else { return };
        if let Some(item) = cell.item.take() {
            for h in cell.handlers.take() {
                item.disconnect(h);
            }
        }
        if let Some(e) = &cell.expander {
            e.set_list_row(None);
        }
    }
}
