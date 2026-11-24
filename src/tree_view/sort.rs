//! Sorting by a column, and the sort indicator of the column headers

use std::cmp::Ordering;

use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

use super::TreeView;
use super::column::COLUMN_HIDE_FROM_TOP_MENU;
use super::value::Val;
use crate::i18n::tr;

pub const SORT_TYPE_ACTION: &str = "sort-type";
pub const SORT_COLUMN_ACTION: &str = "sort-column";

/// Collation of UTF-8 strings, like g_utf8_collate()
fn utf8_collate(a: &str, b: &str) -> Ordering {
    let a = glib::GString::from(a);
    let b = glib::GString::from(b);
    let r = unsafe { glib::ffi::g_utf8_collate(a.as_ptr(), b.as_ptr()) };
    r.cmp(&0)
}

/// A value rows can be sorted by, compared like the cells' values (strings
/// by their collation)
pub trait SortKey {
    fn compare(&self, other: &Self) -> Ordering;
}

impl SortKey for i64 {
    fn compare(&self, other: &Self) -> Ordering {
        self.cmp(other)
    }
}

impl SortKey for u32 {
    fn compare(&self, other: &Self) -> Ordering {
        self.cmp(other)
    }
}

impl SortKey for f64 {
    fn compare(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

impl SortKey for String {
    fn compare(&self, other: &Self) -> Ordering {
        utf8_collate(self, other)
    }
}

impl TreeView {
    pub(super) fn sort_func(&self, a: &glib::Object, b: &glib::Object) -> gtk::Ordering {
        let imp = self.imp();
        let key = imp.sort_column.get();
        if key < 0 {
            return gtk::Ordering::Equal;
        }
        // (without cloning the column, this is called a lot)
        let columns = imp.columns.borrow();
        // a key that isn't registered (from older settings) doesn't sort
        let Some(desc) = columns.iter().find(|d| d.key == key) else {
            return gtk::Ordering::Equal;
        };
        let mut result = match desc.compare.borrow().as_ref() {
            Some(compare) => compare(a, b),
            None => Val::of(a, desc.property).compare(&Val::of(b, desc.property)),
        };
        if imp.sort_order.get() == gtk::SortType::Descending {
            result = result.reverse();
        }
        result.into()
    }

    /// Shows the sort column in the column headers
    pub(super) fn sync_sort_indicator(&self) {
        let imp = self.imp();
        let desc = self.find_key(imp.sort_column.get());
        let column = desc.and_then(|d| d.column.borrow().clone());
        imp.updating_sort.set(true);
        imp.column_view.sort_by_column(column.as_ref(), imp.sort_order.get());
        imp.updating_sort.set(false);
    }

    /// Sorting by the column with key, -1 for the model's order
    pub fn set_sort(&self, column: i32, order: gtk::SortType) {
        let imp = self.imp();
        let column = column.max(-1);
        if imp.sort_column.get() == column && imp.sort_order.get() == order {
            return;
        }
        imp.sort_column.set(column);
        imp.sort_order.set(order);
        self.sync_sort_indicator();
        self.resort();
        self.emit_by_name::<()>("sort-changed", &[]);
    }

    pub fn sort(&self) -> (i32, gtk::SortType) {
        let imp = self.imp();
        (imp.sort_column.get(), imp.sort_order.get())
    }

    /// The items changed, so they may be in a different order now
    pub fn resort(&self) {
        let imp = self.imp();
        if imp.sort_column.get() >= 0
            && let Some(s) = imp.sorter.borrow().as_ref()
        {
            s.changed(gtk::SorterChange::Different);
        }
    }

    /// A column header was clicked
    pub(super) fn column_sorter_changed(&self, sorter: &gtk::Sorter) {
        if self.imp().updating_sort.get() {
            return;
        }
        let Some(cvs) = sorter.downcast_ref::<gtk::ColumnViewSorter>() else { return };
        let desc = cvs.primary_sort_column().and_then(|c| {
            self.imp()
                .columns
                .borrow()
                .iter()
                .find(|d| d.column.borrow().as_ref() == Some(&c))
                .cloned()
        });
        match desc {
            Some(d) => self.set_sort(d.key, cvs.primary_sort_order()),
            None => self.set_sort(-1, gtk::SortType::Ascending),
        }
    }

    pub fn set_sortable(&self, sortable: bool) {
        self.imp().sortable.set(sortable);
    }

    /// Creates the stateful sort actions in map (a window, so "win." actions)
    /// and returns a menu using them.
    pub fn sort_menu(&self, map: &impl IsA<gio::ActionMap>) -> gio::Menu {
        let (sort, sort_type) = self.sort();
        let menu = gio::Menu::new();

        let type_action = gio::SimpleAction::new_stateful(
            SORT_TYPE_ACTION,
            Some(glib::VariantTy::INT32),
            &(sort_type.into_glib()).to_variant(),
        );
        let weak = self.downgrade();
        type_action.connect_change_state(move |_, value| {
            if let (Some(tv), Some(v)) = (weak.upgrade(), value.and_then(|v| v.get::<i32>())) {
                let (column, _) = tv.sort();
                tv.set_sort(column, unsafe { glib::translate::from_glib(v) });
            }
        });
        map.add_action(&type_action);

        let column_action = gio::SimpleAction::new_stateful(
            SORT_COLUMN_ACTION,
            Some(glib::VariantTy::INT32),
            &sort.to_variant(),
        );
        let weak = self.downgrade();
        column_action.connect_change_state(move |_, value| {
            if let (Some(tv), Some(v)) = (weak.upgrade(), value.and_then(|v| v.get::<i32>())) {
                let (_, order) = tv.sort();
                tv.set_sort(v, order);
            }
        });
        map.add_action(&column_action);

        // Keep the radio items in sync when sorting changes elsewhere, eg. by
        // clicking a column header.
        self.connect_sort_changed(move |tv| {
            let (sort, order) = tv.sort();
            type_action.set_state(&order.into_glib().to_variant());
            column_action.set_state(&sort.to_variant());
        });

        let section = gio::Menu::new();
        let item = gio::MenuItem::new(Some(&tr("Ascending")), None);
        item.set_action_and_target_value(
            Some(&format!("win.{SORT_TYPE_ACTION}")),
            Some(&gtk::SortType::Ascending.into_glib().to_variant()),
        );
        section.append_item(&item);
        let item = gio::MenuItem::new(Some(&tr("Descending")), None);
        item.set_action_and_target_value(
            Some(&format!("win.{SORT_TYPE_ACTION}")),
            Some(&gtk::SortType::Descending.into_glib().to_variant()),
        );
        section.append_item(&item);
        menu.append_section(None, &section);

        let section = gio::Menu::new();
        for desc in self.imp().columns.borrow().iter() {
            if desc.flags.get() & COLUMN_HIDE_FROM_TOP_MENU == 0 {
                let item = gio::MenuItem::new(Some(&desc.header), None);
                item.set_action_and_target_value(
                    Some(&format!("win.{SORT_COLUMN_ACTION}")),
                    Some(&desc.key.to_variant()),
                );
                section.append_item(&item);
            }
        }
        menu.append_section(None, &section);

        menu
    }
}
