//! The session (the server's settings) as session_get returns it, with the
//! fields the application asks for (see session_fields()).

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::json::Obj;
use crate::rpc::{Error, Result};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Session {
    /// "4.1.4 (a6d907d9ea)"
    pub version: String,
    pub rpc_version_semver: String,

    pub download_dir: String,
    pub incomplete_dir: String,
    pub incomplete_dir_enabled: bool,
    pub rename_partial_files: bool,
    pub start_added_torrents: bool,
    pub trash_original_torrent_files: bool,
    pub script_torrent_done_enabled: bool,
    pub script_torrent_done_filename: String,
    pub cache_size_mib: i64,
    /// The default for added torrents; missing for servers without
    /// sequential downloading
    pub sequential_download: Option<bool>,

    pub peer_port: i64,
    pub peer_port_random_on_start: bool,
    pub port_forwarding_enabled: bool,
    /// "required", "preferred" or "allowed"
    pub encryption: String,
    /// "utp" and/or "tcp", in the order of preference; missing for older
    /// servers
    pub preferred_transports: Option<Vec<String>>,
    pub pex_enabled: bool,
    pub dht_enabled: bool,
    pub lpd_enabled: bool,
    pub blocklist_enabled: bool,
    pub blocklist_url: String,
    pub blocklist_size: i64,
    pub peer_limit_global: i64,
    pub peer_limit_per_torrent: i64,

    /* KiB/s */
    pub speed_limit_down: i64,
    pub speed_limit_down_enabled: bool,
    pub speed_limit_up: i64,
    pub speed_limit_up_enabled: bool,
    pub alt_speed_enabled: bool,
    pub alt_speed_down: i64,
    pub alt_speed_up: i64,
    /// The alternative speeds at times: from begin to end (minutes after
    /// midnight) on the days (a bit each, Sunday first)
    pub alt_speed_time_enabled: bool,
    pub alt_speed_time_begin: i64,
    pub alt_speed_time_end: i64,
    pub alt_speed_time_day: i64,
    pub seed_ratio_limit: f64,
    pub seed_ratio_limited: bool,

    pub download_queue_enabled: bool,
    pub download_queue_size: i64,
    pub seed_queue_enabled: bool,
    pub seed_queue_size: i64,
    pub queue_stalled_enabled: bool,
    pub queue_stalled_minutes: i64,

    /// Not asked for: the free space of download_dir, which the client gets
    /// with free_space
    #[serde(skip)]
    pub download_dir_free_space: Option<i64>,
}

/// The fields of session_get, the keys of Session
pub fn session_fields() -> &'static [String] {
    static FIELDS: OnceLock<Vec<String>> = OnceLock::new();
    FIELDS.get_or_init(|| match serde_json::to_value(Session::default()) {
        Ok(Value::Object(o)) => o.into_iter().map(|(k, _)| k).collect(),
        _ => Vec::new(),
    })
}

impl Session {
    /// The session of a session_get result
    pub fn of(result: Obj) -> Result<Self> {
        serde_json::from_value(Value::Object(result)).map_err(|e| Error::Decode(e.to_string()))
    }

    /// Its fields as a JSON object, for widgets that load them by their keys
    pub fn to_obj(&self) -> Obj {
        match serde_json::to_value(self) {
            Ok(Value::Object(o)) => o,
            _ => Obj::new(),
        }
    }

    /// Whether the download or seed queue is on
    pub fn queues_enabled(&self) -> bool {
        self.download_queue_enabled || self.seed_queue_enabled
    }
}
