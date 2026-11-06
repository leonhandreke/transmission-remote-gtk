//! The Transmission RPC protocol on the wire, independent of the UI.
//!
//! Transmission 4.1 and later speak JSON-RPC 2.0, with snake_case method
//! names and keys. Older servers aren't supported: they're recognized by the
//! missing `X-Transmission-Rpc-Version` header of their 409 response (or a
//! legacy `{result, arguments}` response), which fails with
//! [`Error::Unsupported`]. [`Transport`] sends requests over HTTP with
//! libsoup, which runs on the glib main loop.
//!
//! JSON-RPC 2.0 allows batches (several requests in one HTTP request, which
//! Transmission executes in order) and notifications (requests without an
//! id, which the server answers with no content).

mod transport;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use transport::{Config, Transport};

pub type Obj = Map<String, Value>;
pub type Result<T = Obj> = std::result::Result<T, Error>;

pub const JSONRPC_VERSION: &str = "2.0";
pub const SESSION_ID_HEADER: &str = "X-Transmission-Session-Id";
/// Sent by Transmission 4.1 and later with the 409 response
pub const RPC_VERSION_HEADER: &str = "X-Transmission-Rpc-Version";
/// The major rpc_version_semver from which servers speak JSON-RPC 2.0
pub const JSONRPC_MIN_MAJOR: u64 = 6;
/// The major rpc_version_semver this was written for. A server with a newer
/// one may have breaking changes.
pub const SUPPORTED_MAJOR: u64 = 6;

/// The error codes of Transmission's JSON-RPC errors (JsonRpc::Error::Code
/// in libtransmission/rpcimpl.h)
#[allow(dead_code)] // the protocol's, not all of which are used
pub mod code {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    pub const SET_ANNOUNCE_LIST: i64 = 1;
    pub const INVALID_TRACKER_LIST: i64 = 2;
    pub const PATH_NOT_ABSOLUTE: i64 = 3;
    pub const UNRECOGNIZED_INFO: i64 = 4;
    pub const SYSTEM_ERROR: i64 = 5;
    pub const FILE_IDX_OOR: i64 = 6;
    pub const PIECE_IDX_OOR: i64 = 7;
    pub const HTTP_ERROR: i64 = 8;
    pub const CORRUPT_TORRENT: i64 = 9;
}

/// A request: a method and its parameters, with the current (snake_case)
/// names
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: &'static str,
    pub params: Obj,
}

impl Request {
    pub fn new(method: &'static str) -> Self {
        Self { method, params: Obj::new() }
    }

    /// Sets a parameter
    pub fn with(mut self, key: &str, value: impl Serialize) -> Self {
        self.set(key, value);
        self
    }

    pub fn set(&mut self, key: &str, value: impl Serialize) {
        self.params.insert(key.to_string(), serde_json::to_value(value).unwrap_or(Value::Null));
    }

    pub fn params_mut(&mut self) -> &mut Obj {
        &mut self.params
    }

    pub fn param(&self, key: &str) -> Option<&Value> {
        self.params.get(key)
    }
}

/// A failed request
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// The HTTP request failed (connection refused, TLS, timeout, ...)
    #[error("{0}")]
    Transport(String),
    /// The server responded with an unexpected HTTP status
    #[error("{}", crate::i18n::sprintf(&crate::i18n::tr("Request failed with HTTP code: %d"), &[&.0]))]
    Status(u16),
    /// A 409 without a session ID, so likely not Transmission
    #[error("No \"{SESSION_ID_HEADER}\" header sent, is this the right host?")]
    NoSessionId,
    /// A Transmission older than 4.1, without JSON-RPC 2.0
    #[error("{}", crate::i18n::tr("This application supports Transmission 4.1 and later."))]
    Unsupported,
    /// The response isn't what it should be
    #[error("{0}")]
    Decode(String),
    /// The server ran the request, and it failed
    #[error("{}", .detail.as_deref().unwrap_or(.message))]
    Rpc {
        /// One of [`code`]
        code: i64,
        message: String,
        /// The specific error message (error.data.error_string)
        detail: Option<String>,
        /// Method specific information (error.data.result)
        result: Option<Box<Obj>>,
    },
}

impl Error {
    /// Whether the server couldn't be reached at all
    pub fn is_transport(&self) -> bool {
        matches!(self, Error::Transport(_))
    }
}

/// Parses "6.0.1" into its numbers
pub fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let mut parts = s.trim().split('.').map(|p| p.parse::<u64>().ok());
    let major = parts.next()??;
    let minor = parts.next().flatten().unwrap_or(0);
    let patch = parts.next().flatten().unwrap_or(0);
    Some((major, minor, patch))
}

/* JSON-RPC 2.0 envelopes */

#[derive(Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'static str,
    method: &'a str,
    params: &'a Obj,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u64>,
}

#[derive(Deserialize)]
struct JsonRpcResponse {
    #[serde(default)]
    jsonrpc: Option<String>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<ErrorObject>,
    #[serde(default)]
    id: Option<Value>,
}

#[derive(Deserialize)]
struct ErrorObject {
    code: i64,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Option<ErrorData>,
}

#[derive(Deserialize, Default)]
struct ErrorData {
    #[serde(default)]
    error_string: Option<String>,
    #[serde(default)]
    result: Option<Obj>,
}

impl From<ErrorObject> for Error {
    fn from(e: ErrorObject) -> Self {
        let data = e.data.unwrap_or_default();
        Error::Rpc {
            code: e.code,
            message: e.message,
            detail: data.error_string,
            result: data.result.map(Box::new),
        }
    }
}

/// A request, or several in a batch, with the ids first_id, first_id + 1,
/// ... or none for notifications
pub(crate) fn encode_jsonrpc(requests: &[Request], first_id: Option<u64>) -> Vec<u8> {
    fn envelope(i: usize, r: &Request, first_id: Option<u64>) -> JsonRpcRequest<'_> {
        JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION,
            method: r.method,
            params: &r.params,
            id: first_id.map(|id| id + i as u64),
        }
    }
    let encoded = if requests.len() == 1 {
        serde_json::to_vec(&envelope(0, &requests[0], first_id))
    } else {
        let all: Vec<_> =
            requests.iter().enumerate().map(|(i, r)| envelope(i, r, first_id)).collect();
        serde_json::to_vec(&all)
    };
    encoded.unwrap_or_default()
}

/// Parses JSON, which Transmission may send as invalid UTF-8 (#261 of the
/// C version)
pub(crate) fn parse_json(bytes: &[u8]) -> std::result::Result<Value, String> {
    match std::str::from_utf8(bytes) {
        Ok(s) => serde_json::from_str(s),
        Err(_) => {
            gtk::glib::g_warning!(
                "trg",
                "Invalid JSON received from Transmission, fixing it, but data may be wrong/corrupted"
            );
            serde_json::from_str(&String::from_utf8_lossy(bytes))
        }
    }
    .map_err(|e| e.to_string())
}

/// The results of the n requests with the ids from first_id, in order
pub(crate) fn decode_jsonrpc(
    bytes: &[u8],
    first_id: u64,
    n: usize,
) -> std::result::Result<Vec<Result>, Error> {
    let value = parse_json(bytes).map_err(Error::Decode)?;

    // an old server ignores "jsonrpc" and fails with an unknown method
    if let Value::Object(o) = &value
        && !o.contains_key("jsonrpc")
        && o.get("result").is_some_and(Value::is_string)
    {
        return Err(Error::Unsupported);
    }

    let responses: Vec<JsonRpcResponse> = match value {
        Value::Array(a) => a.into_iter().map(serde_json::from_value).collect(),
        v => serde_json::from_value(v).map(|r| vec![r]),
    }
    .map_err(|e| Error::Decode(e.to_string()))?;

    let mut results: Vec<Option<Result>> = (0..n).map(|_| None).collect();
    for response in responses {
        if response.jsonrpc.as_deref() != Some(JSONRPC_VERSION) {
            return Err(Error::Decode("Not a JSON-RPC 2.0 response.".into()));
        }
        let result = match (response.result, response.error) {
            (_, Some(error)) => Err(error.into()),
            (Some(Value::Object(mut obj)), None) => {
                normalize_table(&mut obj);
                Ok(obj)
            }
            (_, None) => Ok(Obj::new()),
        };
        let index =
            response.id.as_ref().and_then(Value::as_u64).and_then(|id| id.checked_sub(first_id));
        match index.map(|i| i as usize) {
            Some(i) if i < n => results[i] = Some(result),
            // an error the server couldn't attribute to a request (id null),
            // such as a parse error, applies to all of them
            _ => {
                if let Err(e) = &result {
                    for r in results.iter_mut().filter(|r| r.is_none()) {
                        *r = Some(Err(e.clone()));
                    }
                }
            }
        }
    }

    Ok(results
        .into_iter()
        .map(|r| r.unwrap_or_else(|| Err(Error::Decode("No response to the request.".into()))))
        .collect())
}

/// torrent_get with "format": "table" returns the torrents as rows, the
/// first of which has the keys. Turns them into objects, as with the
/// default format.
pub(crate) fn normalize_table(result: &mut Obj) {
    let Some(Value::Array(rows)) = result.get_mut("torrents") else { return };
    let Some(Value::Array(header)) = rows.first() else { return };
    let keys: Vec<String> =
        header.iter().map(|k| k.as_str().unwrap_or_default().to_string()).collect();
    let objects = rows
        .drain(..)
        .skip(1)
        .filter_map(|row| match row {
            Value::Array(values) => Some(Value::Object(keys.iter().cloned().zip(values).collect())),
            _ => None,
        })
        .collect();
    *rows = objects;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Obj {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn encodes_requests_and_batches() {
        let r = Request::new("torrent_get").with("ids", [1]);
        let single: Value = serde_json::from_slice(&encode_jsonrpc(&[r.clone()], Some(7))).unwrap();
        assert_eq!(
            single,
            json!({"jsonrpc": "2.0", "method": "torrent_get", "params": {"ids": [1]}, "id": 7})
        );

        let batch: Value = serde_json::from_slice(&encode_jsonrpc(
            &[r.clone(), Request::new("session_get")],
            Some(7),
        ))
        .unwrap();
        assert_eq!(batch[1]["id"], 8);
        assert_eq!(batch[1]["method"], "session_get");

        let notification: Value = serde_json::from_slice(&encode_jsonrpc(&[r], None)).unwrap();
        assert!(notification.get("id").is_none());
    }

    #[test]
    fn decodes_results_errors_out_of_order() {
        let body = json!([
            {"jsonrpc": "2.0", "id": 11, "error": {"code": 8, "message": "HTTP error from backend service",
                "data": {"error_string": "Couldn't test port: No Response (0)", "result": {"ip_protocol": "ipv6"}}}},
            {"jsonrpc": "2.0", "id": 10, "result": {"port_is_open": true, "ip_protocol": "ipv4"}},
        ]);
        let results = decode_jsonrpc(&serde_json::to_vec(&body).unwrap(), 10, 2).unwrap();
        assert_eq!(results[0].as_ref().unwrap()["port_is_open"], true);
        match &results[1] {
            Err(e @ Error::Rpc { code, result, .. }) => {
                assert_eq!(*code, code::HTTP_ERROR);
                assert_eq!(result.as_ref().unwrap()["ip_protocol"], "ipv6");
                assert_eq!(e.to_string(), "Couldn't test port: No Response (0)");
            }
            r => panic!("{r:?}"),
        }
    }

    #[test]
    fn error_without_id_applies_to_all() {
        let body = json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "Parse error"}});
        let results = decode_jsonrpc(&serde_json::to_vec(&body).unwrap(), 1, 2).unwrap();
        assert!(
            results.iter().all(|r| matches!(r, Err(Error::Rpc { code: code::PARSE_ERROR, .. })))
        );
    }

    #[test]
    fn detects_old_servers() {
        let body = br#"{"result": "method name not recognized", "arguments": {}}"#;
        assert!(matches!(decode_jsonrpc(body, 1, 1), Err(Error::Unsupported)));
    }

    #[test]
    fn normalizes_tables() {
        let mut result =
            obj(json!({"torrents": [["id", "name"], [1, "a"], [2, "b"]], "removed": [3]}));
        normalize_table(&mut result);
        assert_eq!(result["torrents"], json!([{"id": 1, "name": "a"}, {"id": 2, "name": "b"}]));
        // objects stay as they are
        let mut result = obj(json!({"torrents": [{"id": 1}]}));
        normalize_table(&mut result);
        assert_eq!(result["torrents"], json!([{"id": 1}]));
    }

    #[test]
    fn semver() {
        assert_eq!(parse_semver("6.0.1"), Some((6, 0, 1)));
        assert_eq!(parse_semver("5.3"), Some((5, 3, 0)));
        assert_eq!(parse_semver("x"), None);
    }
}
