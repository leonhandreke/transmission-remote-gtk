//! The values of the cells: properties of the row objects, and their
//! formatting by the type of the column

use std::cmp::Ordering;

use gtk::glib;
use gtk::prelude::*;

use super::ColType;
use super::column::ColumnDesc;
use super::sort::SortKey;
use crate::format;
use crate::i18n::tr;
use crate::protocol::{TR_PRI_HIGH, TR_PRI_LOW, TR_PRI_MIXED, TR_PRI_NORMAL};

/// A value of a cell: a property of the row object, of the types the views
/// show
#[derive(Clone, Debug)]
pub(super) enum Val {
    Empty,
    Str(Option<String>),
    I64(i64),
    U32(u32),
    F64(f64),
    Bool(bool),
}

impl Val {
    /// The value of the property of a row object
    pub(super) fn of(obj: &glib::Object, property: &str) -> Self {
        let value = obj.property_value(property);
        let t = value.type_();
        if t == glib::Type::STRING {
            Val::Str(value.get::<Option<String>>().ok().flatten())
        } else if t == glib::Type::I64 {
            Val::I64(value.get().unwrap_or_default())
        } else if t == glib::Type::I32 {
            Val::I64(value.get::<i32>().unwrap_or_default().into())
        } else if t == glib::Type::U32 {
            Val::U32(value.get().unwrap_or_default())
        } else if t == glib::Type::F64 {
            Val::F64(value.get().unwrap_or_default())
        } else if t == glib::Type::BOOL {
            Val::Bool(value.get().unwrap_or_default())
        } else {
            Val::Empty
        }
    }

    pub(super) fn as_i64(&self) -> i64 {
        match self {
            Val::I64(v) => *v,
            Val::U32(v) => *v as i64,
            Val::F64(v) => *v as i64,
            Val::Bool(v) => *v as i64,
            _ => 0,
        }
    }

    pub(super) fn as_f64(&self) -> f64 {
        match self {
            Val::F64(v) => *v,
            other => other.as_i64() as f64,
        }
    }

    pub(super) fn as_str(&self) -> Option<&str> {
        match self {
            Val::Str(s) => s.as_deref(),
            _ => None,
        }
    }

    /// The value as text, like a GValue transformed to a string
    pub(super) fn to_text(&self) -> Option<String> {
        match self {
            Val::Str(s) => s.clone(),
            Val::I64(v) => Some(v.to_string()),
            Val::U32(v) => Some(v.to_string()),
            Val::F64(v) => Some(format!("{v:.6}")),
            Val::Bool(v) => Some(if *v { "TRUE" } else { "FALSE" }.into()),
            Val::Empty => None,
        }
    }

    /// Compares values of the same type, strings by their collation
    pub(super) fn compare(&self, other: &Val) -> Ordering {
        match (self, other) {
            (Val::Str(a), Val::Str(b)) => match (a, b) {
                (Some(a), Some(b)) => a.compare(b),
                (a, b) => a.is_some().cmp(&b.is_some()),
            },
            (Val::I64(a), Val::I64(b)) => a.cmp(b),
            (Val::U32(a), Val::U32(b)) => a.cmp(b),
            (Val::F64(a), Val::F64(b)) => a.compare(b),
            (Val::Bool(a), Val::Bool(b)) => a.cmp(b),
            _ => Ordering::Equal,
        }
    }
}

/// Value formatting, like the cell renderers of the GtkTreeView did
pub(super) fn format_value(desc: &ColumnDesc, value: &Val) -> Option<String> {
    match desc.ctype {
        ColType::Speed => {
            let speed = value.as_i64();
            (speed > 0).then(|| format::speed(speed))
        }
        ColType::Epoch => {
            let epoch = value.as_i64();
            (epoch > 0).then(|| format::date_time(epoch))
        }
        ColType::Eta => {
            let eta = value.as_i64();
            if eta > 0 {
                Some(format::time_short(eta))
            } else if eta == -2 {
                Some("\u{221E}".into())
            } else {
                None
            }
        }
        ColType::Size => {
            let size = value.as_i64();
            (size > 0).then(|| format::size(size))
        }
        ColType::Ratio => {
            let ratio = value.as_f64();
            (ratio > 0.0).then(|| format::ratio(ratio))
        }
        ColType::Prio => match value.as_i64() {
            TR_PRI_LOW => Some(tr("Low")),
            TR_PRI_HIGH => Some(tr("High")),
            TR_PRI_NORMAL => Some(tr("Normal")),
            TR_PRI_MIXED => Some(tr("Mixed")),
            _ => None,
        },
        ColType::NumGtZero | ColType::NumGtEqZero => {
            let v = value.as_i64();
            let min = if desc.ctype == ColType::NumGtZero { 1 } else { 0 };
            (v >= min).then(|| v.to_string())
        }
        _ => value.to_text(),
    }
}
