//! Saving the columns (their order and widths) and the sorting to the
//! preferences, and restoring them

use gtk::glib;
use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use serde_json::{Value, json};

use super::TreeView;
use super::column::COLUMN_EXTRA;
use crate::json::value_as_i64;
use crate::prefs;

pub const PERSIST_SORT: u32 = 1 << 0;
pub const PERSIST_LAYOUT: u32 = 1 << 1;

impl TreeView {
    fn with_props<R>(&self, f: impl FnOnce(&mut serde_json::Map<String, Value>) -> R) -> Option<R> {
        let prefs = self.prefs()?;
        let id = self.imp().config_id.borrow().clone();
        Some(prefs.with_tree_view_props(&id, f))
    }

    pub fn persist(&self, flags: u32) {
        let (sort_col, sort_type) = self.sort();
        let mut ids = Vec::new();
        let mut widths = Vec::new();
        if flags & PERSIST_LAYOUT != 0 {
            let columns = self.imp().column_view.columns();
            for col in (0..columns.n_items())
                .filter_map(|i| columns.item(i).and_downcast::<gtk::ColumnViewColumn>())
            {
                let desc = self
                    .imp()
                    .columns
                    .borrow()
                    .iter()
                    .find(|d| d.column.borrow().as_ref() == Some(&col))
                    .cloned();
                if let Some(desc) = desc {
                    ids.push(json!(desc.id));
                    // The width is the one the user resized the column to, or -1
                    widths.push(json!(col.fixed_width()));
                }
            }
        }
        self.with_props(|props| {
            if flags & PERSIST_SORT != 0 {
                props.insert(prefs::KEY_TV_SORT_COL.into(), json!(sort_col));
                props.insert(prefs::KEY_TV_SORT_TYPE.into(), json!(sort_type.into_glib()));
            }
            if flags & PERSIST_LAYOUT != 0 {
                props.insert(prefs::KEY_TV_WIDTHS.into(), Value::Array(widths));
                props.insert(prefs::KEY_TV_COLUMNS.into(), Value::Array(ids));
            }
        });
    }

    pub fn restore_sort(&self) {
        let saved = self.with_props(|props| {
            match (props.get(prefs::KEY_TV_SORT_COL), props.get(prefs::KEY_TV_SORT_TYPE)) {
                (Some(c), Some(t)) => Some((value_as_i64(c), value_as_i64(t))),
                _ => None,
            }
        });
        if let Some(Some((col, t))) = saved {
            self.set_sort(col as i32, unsafe { glib::translate::from_glib(t as i32) });
        }
    }

    pub fn setup_columns(&self) {
        let saved = self
            .with_props(|props| {
                match (
                    props.get(prefs::KEY_TV_COLUMNS).and_then(Value::as_array),
                    props.get(prefs::KEY_TV_WIDTHS).and_then(Value::as_array),
                ) {
                    (Some(c), Some(w)) => Some((c.clone(), w.clone())),
                    _ => None,
                }
            })
            .flatten();

        match saved {
            None => {
                let columns = self.imp().columns.borrow().clone();
                for desc in columns.iter().filter(|d| d.flags.get() & COLUMN_EXTRA == 0) {
                    self.add_column_after(desc, -1, None);
                }
            }
            Some((columns, widths)) => {
                for (c, w) in columns.iter().zip(widths.iter()) {
                    if let Some(desc) = c.as_str().and_then(|id| self.find_column(id))
                        && !desc.showing()
                    {
                        self.add_column_after(&desc, value_as_i64(w), None);
                    }
                }
            }
        }

        self.sync_sort_indicator();
        self.update_column_menus();
    }
}
