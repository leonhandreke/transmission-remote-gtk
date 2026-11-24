//! Reading JSON values leniently, like json-glib did in the C version: a
//! missing or mistyped value is 0, false or None. Used for the server's
//! responses and the configuration alike.

use serde_json::{Map, Value};

/// A JSON object
pub type Obj = Map<String, Value>;

/// Integer value of a JSON node, like json_node_get_int()
pub fn value_as_i64(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0),
        Value::Bool(b) => *b as i64,
        _ => 0,
    }
}

/// Floating point value of a JSON node, like json_node_get_double()
pub fn value_as_f64(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::Bool(b) => *b as i64 as f64,
        _ => 0.0,
    }
}

pub fn value_as_bool(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => false,
    }
}

/* The values of an object (0, false or None when missing) */

pub fn get_i64(o: &Obj, key: &str) -> i64 {
    o.get(key).map(value_as_i64).unwrap_or(0)
}

pub fn get_f64(o: &Obj, key: &str) -> f64 {
    o.get(key).map(value_as_f64).unwrap_or(0.0)
}

pub fn get_bool(o: &Obj, key: &str) -> bool {
    o.get(key).map(value_as_bool).unwrap_or(false)
}

pub fn get_str<'a>(o: &'a Obj, key: &str) -> Option<&'a str> {
    o.get(key).and_then(Value::as_str)
}

pub fn get_obj<'a>(o: &'a Obj, key: &str) -> Option<&'a Obj> {
    o.get(key).and_then(Value::as_object)
}

pub fn objects(array: &[Value]) -> impl Iterator<Item = &Obj> {
    array.iter().filter_map(Value::as_object)
}
