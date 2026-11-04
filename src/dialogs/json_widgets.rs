//! Widgets that load and save their state from and to a JSON object, used by
//! the torrent properties and remote settings dialogs (trg-json-widgets.c):
//! a check button is a boolean, a spin button an integer (or a double if it
//! has digits) and other editables a string.

use gtk::prelude::*;
use serde_json::json;

use crate::protocol::{Obj, get_bool, get_f64, get_i64, get_str};

pub struct JsonWidget {
    pub widget: gtk::Widget,
    pub key: String,
}

#[derive(Default)]
pub struct JsonWidgets(Vec<JsonWidget>, Vec<(String, Box<dyn Fn() -> serde_json::Value>)>);

impl JsonWidgets {
    /// Loads a value into a widget, and saves it with the others
    pub fn add(&mut self, w: &impl IsA<gtk::Widget>, obj: &Obj, key: &str) {
        let w = w.as_ref();
        if let Some(check) = w.downcast_ref::<gtk::CheckButton>() {
            check.set_active(get_bool(obj, key));
        } else if let Some(spin) = w.downcast_ref::<gtk::SpinButton>() {
            if spin.digits() == 0 {
                spin.set_value(get_i64(obj, key) as f64);
            } else {
                spin.set_value(get_f64(obj, key));
            }
        } else if let Some(e) = w.dynamic_cast_ref::<gtk::Editable>() {
            e.set_text(get_str(obj, key).unwrap_or(""));
        }
        self.0.push(JsonWidget { widget: w.clone(), key: key.to_string() });
    }

    /// A value saved by a function, for widgets that don't map to a value
    /// directly
    pub fn add_custom(&mut self, key: &str, save: impl Fn() -> serde_json::Value + 'static) {
        self.1.push((key.to_string(), Box::new(save)));
    }

    pub fn save(&self, out: &mut Obj) {
        for (key, f) in &self.1 {
            out.insert(key.clone(), f());
        }
        for wd in &self.0 {
            let w = &wd.widget;
            let value = if let Some(check) = w.downcast_ref::<gtk::CheckButton>() {
                json!(check.is_active())
            } else if let Some(spin) = w.downcast_ref::<gtk::SpinButton>() {
                if spin.digits() == 0 { json!(spin.value_as_int()) } else { json!(spin.value()) }
            } else if let Some(e) = w.dynamic_cast_ref::<gtk::Editable>() {
                json!(e.text().as_str())
            } else {
                continue;
            };
            out.insert(wd.key.clone(), value);
        }
    }
}
