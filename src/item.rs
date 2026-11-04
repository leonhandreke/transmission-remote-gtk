//! The items (rows) of the list models, like TrgItem in the C version: an
//! item has a fixed number of columns, each holding a value of one type.
//! Setting a column to a different value emits "column-changed" with the
//! detail "cN" (N being the column), which the cells of the views watch.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use gtk::gio;
use gtk::glib;
use gtk::glib::subclass::prelude::*;
use gtk::prelude::*;

use crate::protocol::Obj;

/// A value of a column
#[derive(Clone, Debug, Default)]
pub enum Val {
    #[default]
    Empty,
    Str(Option<String>),
    I64(i64),
    U32(u32),
    F64(f64),
    Bool(bool),
    Json(Option<Rc<Obj>>),
}

impl Val {
    pub fn as_i64(&self) -> i64 {
        match self {
            Val::I64(v) => *v,
            Val::U32(v) => *v as i64,
            Val::F64(v) => *v as i64,
            Val::Bool(v) => *v as i64,
            _ => 0,
        }
    }

    pub fn as_f64(&self) -> f64 {
        match self {
            Val::F64(v) => *v,
            other => other.as_i64() as f64,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Val::Str(s) => s.as_deref(),
            _ => None,
        }
    }

    /// The value as text, like a GValue transformed to a string
    pub fn to_text(&self) -> Option<String> {
        match self {
            Val::Str(s) => s.clone(),
            Val::I64(v) => Some(v.to_string()),
            Val::U32(v) => Some(v.to_string()),
            Val::F64(v) => Some(format!("{v:.6}")),
            Val::Bool(v) => Some(if *v { "TRUE" } else { "FALSE" }.into()),
            _ => None,
        }
    }

    fn same(&self, other: &Val) -> bool {
        match (self, other) {
            (Val::Str(a), Val::Str(b)) => a == b,
            (Val::I64(a), Val::I64(b)) => a == b,
            (Val::U32(a), Val::U32(b)) => a == b,
            (Val::F64(a), Val::F64(b)) => a == b,
            (Val::Bool(a), Val::Bool(b)) => a == b,
            (Val::Json(a), Val::Json(b)) => match (a, b) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            },
            (Val::Empty, Val::Empty) => true,
            _ => false,
        }
    }
}

impl From<&str> for Val {
    fn from(s: &str) -> Self {
        Val::Str(Some(s.to_string()))
    }
}

impl From<String> for Val {
    fn from(s: String) -> Self {
        Val::Str(Some(s))
    }
}

impl From<Option<String>> for Val {
    fn from(s: Option<String>) -> Self {
        Val::Str(s)
    }
}

impl From<Option<&str>> for Val {
    fn from(s: Option<&str>) -> Self {
        Val::Str(s.map(str::to_string))
    }
}

impl From<i64> for Val {
    fn from(v: i64) -> Self {
        Val::I64(v)
    }
}

impl From<u32> for Val {
    fn from(v: u32) -> Self {
        Val::U32(v)
    }
}

impl From<f64> for Val {
    fn from(v: f64) -> Self {
        Val::F64(v)
    }
}

impl From<bool> for Val {
    fn from(v: bool) -> Self {
        Val::Bool(v)
    }
}

impl From<Rc<Obj>> for Val {
    fn from(v: Rc<Obj>) -> Self {
        Val::Json(Some(v))
    }
}

/// Collation of UTF-8 strings, like g_utf8_collate()
pub fn utf8_collate(a: &str, b: &str) -> Ordering {
    let a = glib::GString::from(a);
    let b = glib::GString::from(b);
    let r = unsafe { glib::ffi::g_utf8_collate(a.as_ptr(), b.as_ptr()) };
    r.cmp(&0)
}

mod imp {
    use super::*;
    use std::sync::OnceLock;

    #[derive(Default)]
    pub struct Item {
        pub values: RefCell<Vec<Val>>,
        /// The children of a row of a tree (a directory of the files)
        pub children: RefCell<Option<gio::ListStore>>,
        /// The parent of a row of a tree (not a reference, the parent owns
        /// its children)
        pub parent: glib::WeakRef<super::Item>,
        /// The column whose cell should start editing once it's shown
        pub start_edit: std::cell::Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Item {
        const NAME: &'static str = "TrgItem";
        type Type = super::Item;
    }

    impl ObjectImpl for Item {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("column-changed")
                        .param_types([u32::static_type()])
                        .flags(glib::SignalFlags::DETAILED | glib::SignalFlags::RUN_LAST)
                        .build(),
                ]
            })
        }
    }
}

glib::wrapper! {
    pub struct Item(ObjectSubclass<imp::Item>);
}

impl Item {
    /// An item with the given (initial) values of its columns
    pub fn new(values: Vec<Val>) -> Self {
        let item: Self = glib::Object::new();
        *item.imp().values.borrow_mut() = values;
        item.imp().start_edit.set(-1);
        item
    }

    pub fn n_columns(&self) -> usize {
        self.imp().values.borrow().len()
    }

    pub fn get(&self, column: usize) -> Val {
        self.imp().values.borrow().get(column).cloned().unwrap_or_default()
    }

    pub fn with<R>(&self, column: usize, f: impl FnOnce(&Val) -> R) -> R {
        let values = self.imp().values.borrow();
        f(values.get(column).unwrap_or(&Val::Empty))
    }

    pub fn string(&self, column: usize) -> Option<String> {
        self.with(column, |v| v.as_str().map(str::to_string))
    }

    pub fn int64(&self, column: usize) -> i64 {
        self.with(column, Val::as_i64)
    }

    pub fn uint(&self, column: usize) -> u32 {
        self.with(column, |v| v.as_i64() as u32)
    }

    pub fn double(&self, column: usize) -> f64 {
        self.with(column, Val::as_f64)
    }

    pub fn boolean(&self, column: usize) -> bool {
        self.with(column, |v| v.as_i64() != 0)
    }

    pub fn json(&self, column: usize) -> Option<Rc<Obj>> {
        self.with(column, |v| match v {
            Val::Json(j) => j.clone(),
            _ => None,
        })
    }

    /// Sets a column, notifying if the value changed
    pub fn set(&self, column: usize, value: impl Into<Val>) {
        let value = value.into();
        {
            let mut values = self.imp().values.borrow_mut();
            let Some(current) = values.get_mut(column) else {
                return;
            };
            if current.same(&value) {
                return;
            }
            *current = value;
        }
        self.emit_by_name_with_details::<()>(
            "column-changed",
            glib::Quark::from_str(format!("c{column}")),
            &[&(column as u32)],
        );
    }

    /// Calls f when column changes
    pub fn connect_column_changed<F: Fn(&Self) + 'static>(
        &self,
        column: usize,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local(&format!("column-changed::c{column}"), false, move |args| {
            f(&args[0].get::<Self>().unwrap());
            None
        })
    }

    pub fn children(&self) -> Option<gio::ListStore> {
        self.imp().children.borrow().clone()
    }

    pub fn set_children(&self, children: Option<gio::ListStore>) {
        *self.imp().children.borrow_mut() = children;
    }

    pub fn parent_item(&self) -> Option<Item> {
        self.imp().parent.upgrade()
    }

    pub fn set_parent_item(&self, parent: Option<&Item>) {
        self.imp().parent.set(parent);
    }

    pub fn start_edit_column(&self) -> i32 {
        self.imp().start_edit.get()
    }

    pub fn set_start_edit_column(&self, column: i32) {
        self.imp().start_edit.set(column);
    }

    /// Compares the values of a column, strings by their collation
    pub fn compare(&self, other: &Item, column: usize) -> Ordering {
        let a = self.imp().values.borrow();
        let b = other.imp().values.borrow();
        let (Some(va), Some(vb)) = (a.get(column), b.get(column)) else {
            return Ordering::Equal;
        };
        match (va, vb) {
            (Val::Str(sa), Val::Str(sb)) => match (sa, sb) {
                (Some(sa), Some(sb)) => utf8_collate(sa, sb),
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
            (Val::I64(x), Val::I64(y)) => x.cmp(y),
            (Val::U32(x), Val::U32(y)) => x.cmp(y),
            (Val::F64(x), Val::F64(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
            (Val::Bool(x), Val::Bool(y)) => x.cmp(y),
            _ => Ordering::Equal,
        }
    }
}

/// The items of a list model
pub fn items(model: &impl IsA<gio::ListModel>) -> impl Iterator<Item = Item> + '_ {
    let n = model.n_items();
    (0..n).filter_map(move |i| model.item(i).and_downcast::<Item>())
}

/// Removes the items whose serial column isn't current_serial, in runs, so
/// it's linear in the number of items. Returns how many were removed.
pub fn remove_removed(store: &gio::ListStore, serial_column: usize, current_serial: i64) -> u32 {
    remove_where(store, |item| item.int64(serial_column) != current_serial)
}

/// Removes the items for which stale is true, in runs from the end
pub fn remove_where(store: &gio::ListStore, mut stale: impl FnMut(&Item) -> bool) -> u32 {
    let mut removed = 0;
    let mut i = store.n_items();
    while i > 0 {
        let end = i;
        while i > 0 {
            let item = store.item(i - 1).and_downcast::<Item>().unwrap();
            if !stale(&item) {
                break;
            }
            i -= 1;
        }
        if i < end {
            store.splice(i, end - i, &[] as &[glib::Object]);
            removed += end - i;
        } else {
            i -= 1;
        }
    }
    removed
}

/// The first item whose (integer) column is id, and its position
pub fn find_item(model: &impl IsA<gio::ListModel>, column: usize, id: i64) -> Option<(Item, u32)> {
    let n = model.n_items();
    (0..n).find_map(|i| {
        let item = model.item(i).and_downcast::<Item>()?;
        (item.int64(column) == id).then_some((item, i))
    })
}

/// The item of a row: the row itself, or the item of a tree list row
pub fn row_item(row: &glib::Object) -> Option<Item> {
    if let Some(r) = row.downcast_ref::<gtk::TreeListRow>() {
        r.item().and_downcast::<Item>()
    } else {
        row.clone().downcast::<Item>().ok()
    }
}
