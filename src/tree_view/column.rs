//! The columns: registering them, showing and hiding them (with the menus
//! of their headers)

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

use super::TreeView;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColType {
    IconText,
    FileIconText,
    Wanted,
    Text,
    Size,
    Ratio,
    Epoch,
    Speed,
    Eta,
    Prog,
    Prio,
    NumGtZero,
    NumGtEqZero,
}

pub const COLUMN_SHOWING: u32 = 1 << 0;
pub const COLUMN_UNREMOVABLE: u32 = 1 << 1;
pub const COLUMN_EXTRA: u32 = 1 << 2;
pub const COLUMN_HIDE_FROM_TOP_MENU: u32 = 1 << 3;
pub const COLUMN_EDITABLE: u32 = 1 << 4;

/// Called when the user toggles a "wanted" cell or clicks a priority cell
pub type ActivateFn = Rc<dyn Fn(&glib::Object)>;
/// Called when the user starts editing an editable text cell (done is false,
/// new_text None), and when they finish (done is true, new_text is None if
/// the text wasn't changed).
pub type EditFn = Rc<dyn Fn(&glib::Object, bool, Option<&str>)>;
/// Compares two rows by a column, for sorting without reading properties
pub type CompareFn = Rc<dyn Fn(&glib::Object, &glib::Object) -> Ordering>;

pub struct ColumnDesc {
    /// What sorting by the column is saved as (see the top)
    pub key: i32,
    /// The property of the row objects it shows
    pub property: &'static str,
    /// The property of the icon name of an IconText column, the file ID of a
    /// FileIconText one
    pub extra: Cell<Option<&'static str>>,
    pub header: String,
    pub id: String,
    pub flags: Cell<u32>,
    pub ctype: ColType,
    pub activate: RefCell<Option<ActivateFn>>,
    pub edit: RefCell<Option<EditFn>>,
    /// How to sort by it, if not by comparing its property values
    pub compare: RefCell<Option<CompareFn>>,
    pub editable: Cell<bool>,
    pub(super) column: RefCell<Option<gtk::ColumnViewColumn>>,
}

impl ColumnDesc {
    /// The property of the icon name of an IconText column, the file ID of a
    /// FileIconText one
    pub fn set_extra(&self, property: &'static str) {
        self.extra.set(Some(property));
    }

    /// Sets the activate function, for rows of type T
    pub fn set_activate<T: IsA<glib::Object>>(&self, f: impl Fn(&T) + 'static) {
        *self.activate.borrow_mut() = Some(Rc::new(move |obj| {
            if let Some(obj) = obj.downcast_ref::<T>() {
                f(obj);
            }
        }));
    }

    /// Sets the edit function, for rows of type T
    pub fn set_edit<T: IsA<glib::Object>>(&self, f: impl Fn(&T, bool, Option<&str>) + 'static) {
        *self.edit.borrow_mut() = Some(Rc::new(move |obj, done, new_text| {
            if let Some(obj) = obj.downcast_ref::<T>() {
                f(obj, done, new_text);
            }
        }));
    }

    /// Sorts rows of type T by it with f, which reads their values directly
    /// (reading two properties for each comparison is slow for long lists)
    pub fn set_compare<T: IsA<glib::Object>>(&self, f: impl Fn(&T, &T) -> Ordering + 'static) {
        *self.compare.borrow_mut() =
            Some(Rc::new(move |a, b| match (a.downcast_ref::<T>(), b.downcast_ref::<T>()) {
                (Some(a), Some(b)) => f(a, b),
                _ => Ordering::Equal,
            }));
    }

    pub fn set_editable(&self, editable: bool) {
        self.editable.set(editable);
    }

    pub(super) fn showing(&self) -> bool {
        self.flags.get() & COLUMN_SHOWING != 0
    }
}

impl TreeView {
    /// Registers a column of a property of the row objects. The key is what
    /// sorting by it is saved as.
    pub fn reg_property(
        &self,
        ctype: ColType,
        key: i32,
        property: &'static str,
        header: &str,
        id: &str,
        flags: u32,
    ) -> Rc<ColumnDesc> {
        let desc = Rc::new(ColumnDesc {
            key,
            property,
            extra: Cell::new(None),
            header: header.to_string(),
            id: id.to_string(),
            flags: Cell::new(flags),
            ctype,
            activate: Default::default(),
            edit: Default::default(),
            compare: Default::default(),
            editable: Cell::new(flags & COLUMN_EDITABLE != 0),
            column: Default::default(),
        });
        self.imp().columns.borrow_mut().push(desc.clone());
        desc
    }

    pub(super) fn find_column(&self, id: &str) -> Option<Rc<ColumnDesc>> {
        self.imp().columns.borrow().iter().find(|d| d.id == id).cloned()
    }

    pub(super) fn find_key(&self, key: i32) -> Option<Rc<ColumnDesc>> {
        self.imp().columns.borrow().iter().find(|d| d.key == key).cloned()
    }

    /// Whether the column with the (string) ID is showing
    pub fn is_column_showing(&self, id: &str) -> bool {
        self.find_column(id).is_some_and(|d| d.showing())
    }

    fn count_showing(&self) -> usize {
        self.imp().columns.borrow().iter().filter(|d| d.showing()).count()
    }

    fn hide_column(&self, desc: &ColumnDesc) {
        let Some(column) = desc.column.take() else { return };
        self.imp().column_view.remove_column(&column);
        desc.flags.set(desc.flags.get() & !COLUMN_SHOWING);
    }

    fn column_action(&self, desc: &Rc<ColumnDesc>) -> gio::SimpleAction {
        let imp = self.imp();
        let name = format!("visible-{}", desc.id);
        if let Some(action) = imp.actions.lookup_action(&name).and_downcast::<gio::SimpleAction>() {
            return action;
        }
        let action = gio::SimpleAction::new_stateful(&name, None, &false.to_variant());
        let weak = self.downgrade();
        let d = Rc::downgrade(desc);
        action.connect_activate(move |_, _| {
            let (Some(tv), Some(desc)) = (weak.upgrade(), d.upgrade()) else { return };
            if desc.showing() {
                tv.hide_column(&desc);
                tv.sync_sort_indicator();
                tv.update_column_menus();
            }
        });
        imp.actions.add_action(&action);
        action
    }

    /// The menu of each column header has the column itself, checked, which
    /// hides it, and then the hidden columns, which show themselves after it.
    pub(super) fn update_column_menus(&self) {
        let n_showing = self.count_showing();
        let columns = self.imp().columns.borrow().clone();

        for desc in &columns {
            let action = self.column_action(desc);
            let showing = desc.showing();
            action.set_state(&showing.to_variant());
            action.set_enabled(
                showing && desc.flags.get() & COLUMN_UNREMOVABLE == 0 && n_showing > 1,
            );

            let Some(column) = desc.column.borrow().clone() else { continue };

            if !self.imp().column_menus.get() {
                column.set_header_menu(None::<&gio::MenuModel>);
                continue;
            }

            let menu = gio::Menu::new();
            menu.append(Some(&desc.header), Some(&format!("tvcol.visible-{}", desc.id)));
            for hidden in columns.iter().filter(|d| !d.showing()) {
                let item = gio::MenuItem::new(Some(&hidden.header), None);
                item.set_action_and_target_value(
                    Some("tvcol.show-after"),
                    Some(&(hidden.id.as_str(), desc.id.as_str()).to_variant()),
                );
                menu.append_item(&item);
            }
            column.set_header_menu(Some(&menu));
        }
    }

    pub fn set_column_menus(&self, enabled: bool) {
        self.imp().column_menus.set(enabled);
        self.update_column_menus();
    }

    pub(super) fn add_column_after(
        &self,
        desc: &Rc<ColumnDesc>,
        width: i64,
        after: Option<&Rc<ColumnDesc>>,
    ) {
        let imp = self.imp();
        let cv = &imp.column_view;
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
        let weak = self.downgrade();
        factory.connect_unbind(move |_, obj| {
            if let (Some(tv), Some(li)) = (weak.upgrade(), obj.downcast_ref::<gtk::ListItem>()) {
                tv.cell_unbind(li);
            }
        });

        let column = gtk::ColumnViewColumn::new(Some(&desc.header), Some(factory));
        column.set_id(Some(&desc.id));
        column.set_resizable(true);
        column.set_expand(true);
        if width > 0 {
            column.set_fixed_width(width as i32);
        }

        // The column view's sorter only shows the sort column (and makes the
        // headers clickable), the view's own sorter sorts.
        if imp.sortable.get() {
            column.set_sorter(Some(&gtk::CustomSorter::new(|_, _| gtk::Ordering::Equal)));
        }

        let columns = cv.columns();
        let mut position = columns.n_items();
        if let Some(after_col) = after.and_then(|a| a.column.borrow().clone())
            && let Some(i) = (0..columns.n_items())
                .find(|&i| columns.item(i).as_ref() == Some(after_col.upcast_ref()))
        {
            position = i + 1;
        }

        cv.insert_column(position, &column);
        *desc.column.borrow_mut() = Some(column);
        desc.flags.set(desc.flags.get() | COLUMN_SHOWING);
    }

    pub fn remove_all_columns(&self) {
        let columns = self.imp().columns.borrow().clone();
        for desc in &columns {
            self.hide_column(desc);
        }
    }
}
