//! The Transmission RPC protocol (4.1 and later, see docs/rpc-spec.md of
//! Transmission): the method names and keys, the requests, and the results,
//! such as the torrents (see torrent.rs). The wire format is in rpc.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::json::{Obj, get_obj, value_as_i64};

mod session;
mod torrent;

pub use crate::rpc::Request;
pub use session::*;
pub use torrent::*;

pub const FIELD_RECENTLY_ACTIVE: &str = "recently_active";
pub const FIELD_NAME: &str = "name";
pub const FIELD_PATH: &str = "path";
pub const FIELD_MOVE: &str = "move";
pub const FIELD_LOCATION: &str = "location";
pub const FIELD_BANDWIDTH_PRIORITY: &str = "bandwidth_priority";
pub const FIELD_UPLOAD_LIMIT: &str = "upload_limit";
pub const FIELD_UPLOAD_LIMITED: &str = "upload_limited";
pub const FIELD_DOWNLOAD_LIMIT: &str = "download_limit";
pub const FIELD_DOWNLOAD_LIMITED: &str = "download_limited";
pub const FIELD_HONORS_SESSION_LIMITS: &str = "honors_session_limits";
pub const FIELD_SEED_RATIO_MODE: &str = "seed_ratio_mode";
pub const FIELD_SEED_RATIO_LIMIT: &str = "seed_ratio_limit";
pub const FIELD_PEER_LIMIT: &str = "peer_limit";
pub const FIELD_FILE_DOWNLOAD_DIR: &str = "download_dir";
pub const FIELD_QUEUE_POSITION: &str = "queue_position";
pub const FIELD_FILES_WANTED: &str = "files_wanted";
pub const FIELD_FILES_UNWANTED: &str = "files_unwanted";
pub const FIELD_FILES_PRIORITY_HIGH: &str = "priority_high";
pub const FIELD_FILES_PRIORITY_NORMAL: &str = "priority_normal";
pub const FIELD_FILES_PRIORITY_LOW: &str = "priority_low";
pub const FIELD_SEQUENTIAL_DOWNLOAD: &str = "sequential_download";
pub const FIELD_SEQUENTIAL_DOWNLOAD_FROM_PIECE: &str = "sequential_download_from_piece";
/// The announce URLs, one per line, with an empty line between tiers
pub const FIELD_TRACKER_LIST: &str = "tracker_list";
pub const FIELD_TORRENT_ADDED: &str = "torrent_added";
pub const FIELD_TORRENT_DUPLICATE: &str = "torrent_duplicate";
pub const FIELD_FORMAT: &str = "format";
pub const FORMAT_TABLE: &str = "table";

pub const METHOD_TORRENT_START: &str = "torrent_start";
pub const METHOD_SESSION_GET: &str = "session_get";
pub const METHOD_SESSION_SET: &str = "session_set";
pub const METHOD_TORRENT_GET: &str = "torrent_get";
pub const METHOD_TORRENT_SET: &str = "torrent_set";
pub const METHOD_TORRENT_SET_LOCATION: &str = "torrent_set_location";
pub const METHOD_TORRENT_RENAME_PATH: &str = "torrent_rename_path";
pub const METHOD_TORRENT_STOP: &str = "torrent_stop";
pub const METHOD_TORRENT_VERIFY: &str = "torrent_verify";
pub const METHOD_TORRENT_REMOVE: &str = "torrent_remove";
pub const METHOD_TORRENT_ADD: &str = "torrent_add";
pub const METHOD_TORRENT_REANNOUNCE: &str = "torrent_reannounce";
pub const METHOD_PORT_TEST: &str = "port_test";
pub const METHOD_BLOCKLIST_UPDATE: &str = "blocklist_update";
pub const METHOD_SESSION_STATS: &str = "session_stats";
pub const METHOD_QUEUE_MOVE_TOP: &str = "queue_move_top";
pub const METHOD_QUEUE_MOVE_UP: &str = "queue_move_up";
pub const METHOD_QUEUE_MOVE_BOTTOM: &str = "queue_move_bottom";
pub const METHOD_QUEUE_MOVE_DOWN: &str = "queue_move_down";
pub const METHOD_TORRENT_START_NOW: &str = "torrent_start_now";

pub const METHOD_FREE_SPACE: &str = "free_space";

pub const PARAM_IDS: &str = "ids";
pub const PARAM_DELETE_LOCAL_DATA: &str = "delete_local_data";
pub const PARAM_FIELDS: &str = "fields";
pub const PARAM_METAINFO: &str = "metainfo";
pub const PARAM_FILENAME: &str = "filename";
pub const PARAM_PAUSED: &str = "paused";
pub const PARAM_PATH: &str = "path";
pub const PARAM_IP_PROTOCOL: &str = "ip_protocol";
pub const IP_PROTOCOL_IPV4: &str = "ipv4";
pub const IP_PROTOCOL_IPV6: &str = "ipv6";

/// Not in the protocol, just used in the UI
pub const TR_PRI_UNSET: i64 = -3;
pub const TR_PRI_MIXED: i64 = -2;
pub const TR_PRI_LOW: i64 = -1;
pub const TR_PRI_NORMAL: i64 = 0;
pub const TR_PRI_HIGH: i64 = 1;

/* The keys of the session (for session_set) */
pub const SGET_BLOCKLIST_ENABLED: &str = "blocklist_enabled";
pub const SGET_BLOCKLIST_URL: &str = "blocklist_url";
pub const SGET_DHT_ENABLED: &str = "dht_enabled";
pub const SGET_LPD_ENABLED: &str = "lpd_enabled";
pub const SGET_DOWNLOAD_DIR: &str = "download_dir";
pub const SGET_INCOMPLETE_DIR: &str = "incomplete_dir";
pub const SGET_INCOMPLETE_DIR_ENABLED: &str = "incomplete_dir_enabled";
pub const SGET_ENCRYPTION: &str = "encryption";
pub const SGET_PEER_LIMIT_GLOBAL: &str = "peer_limit_global";
pub const SGET_PEER_LIMIT_PER_TORRENT: &str = "peer_limit_per_torrent";
pub const SGET_PEER_PORT: &str = "peer_port";
pub const SGET_PEER_PORT_RANDOM_ON_START: &str = "peer_port_random_on_start";
pub const SGET_PEX_ENABLED: &str = "pex_enabled";
pub const SGET_PORT_FORWARDING_ENABLED: &str = "port_forwarding_enabled";
pub const SGET_SEED_RATIO_LIMIT: &str = "seed_ratio_limit";
pub const SGET_SEED_RATIO_LIMITED: &str = "seed_ratio_limited";
pub const SGET_SPEED_LIMIT_DOWN: &str = "speed_limit_down";
pub const SGET_SPEED_LIMIT_DOWN_ENABLED: &str = "speed_limit_down_enabled";
pub const SGET_SPEED_LIMIT_UP: &str = "speed_limit_up";
pub const SGET_SPEED_LIMIT_UP_ENABLED: &str = "speed_limit_up_enabled";
pub const SGET_TRASH_ORIGINAL_TORRENT_FILES: &str = "trash_original_torrent_files";
pub const SGET_START_ADDED_TORRENTS: &str = "start_added_torrents";
pub const SGET_RENAME_PARTIAL_FILES: &str = "rename_partial_files";
pub const SGET_CACHE_SIZE_MIB: &str = "cache_size_mib";
pub const SGET_SCRIPT_TORRENT_DONE_FILENAME: &str = "script_torrent_done_filename";
pub const SGET_SCRIPT_TORRENT_DONE_ENABLED: &str = "script_torrent_done_enabled";
pub const SGET_DOWNLOAD_QUEUE_ENABLED: &str = "download_queue_enabled";
pub const SGET_DOWNLOAD_QUEUE_SIZE: &str = "download_queue_size";
pub const SGET_SEED_QUEUE_ENABLED: &str = "seed_queue_enabled";
pub const SGET_SEED_QUEUE_SIZE: &str = "seed_queue_size";
pub const SGET_QUEUE_STALLED_ENABLED: &str = "queue_stalled_enabled";
pub const SGET_QUEUE_STALLED_MINUTES: &str = "queue_stalled_minutes";
pub const SGET_ALT_SPEED_DOWN: &str = "alt_speed_down";
pub const SGET_ALT_SPEED_ENABLED: &str = "alt_speed_enabled";
pub const SGET_ALT_SPEED_TIME_BEGIN: &str = "alt_speed_time_begin";
pub const SGET_ALT_SPEED_TIME_ENABLED: &str = "alt_speed_time_enabled";
pub const SGET_ALT_SPEED_TIME_END: &str = "alt_speed_time_end";
pub const SGET_ALT_SPEED_TIME_DAY: &str = "alt_speed_time_day";
pub const SGET_ALT_SPEED_UP: &str = "alt_speed_up";
pub const SGET_SEQUENTIAL_DOWNLOAD: &str = "sequential_download";
/// "utp" and/or "tcp", in the order of preference
pub const SGET_PREFERRED_TRANSPORTS: &str = "preferred_transports";
pub const TRANSPORT_UTP: &str = "utp";
pub const TRANSPORT_TCP: &str = "tcp";
pub const ENCRYPTION_REQUIRED: &str = "required";
pub const ENCRYPTION_PREFERRED: &str = "preferred";
pub const ENCRYPTION_ALLOWED: &str = "allowed";

/* Requests */

/// The torrents of a request
#[derive(Debug, Clone, PartialEq)]
pub enum Torrents {
    All,
    /// The ones changed recently, with the IDs of the removed ones
    RecentlyActive,
    Ids(Vec<i64>),
}

impl Torrents {
    /// The torrents of a request's "ids" parameter
    pub fn of(req: &Request) -> Self {
        match req.param(PARAM_IDS) {
            Some(Value::Array(ids)) => Torrents::Ids(ids.iter().map(value_as_i64).collect()),
            Some(Value::String(s)) if s == FIELD_RECENTLY_ACTIVE => Torrents::RecentlyActive,
            _ => Torrents::All,
        }
    }

    fn set_on(&self, req: &mut Request) {
        match self {
            Torrents::All => {}
            Torrents::RecentlyActive => req.set(PARAM_IDS, FIELD_RECENTLY_ACTIVE),
            Torrents::Ids(ids) => req.set(PARAM_IDS, ids),
        }
    }
}

fn request(method: &'static str, ids: Option<&[i64]>) -> Request {
    let mut req = Request::new(method);
    if let Some(ids) = ids {
        req.set(PARAM_IDS, ids);
    }
    req
}

/// A result as T, or T's default if it doesn't fit
pub fn parse<T: DeserializeOwned + Default>(result: &Obj) -> T {
    serde_json::from_value(Value::Object(result.clone())).unwrap_or_default()
}

pub fn session_stats() -> Request {
    Request::new(METHOD_SESSION_STATS)
}

/// The result of session_stats
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SessionStats {
    /// Since the server started
    pub current_stats: TransferStats,
    /// Ever
    pub cumulative_stats: TransferStats,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TransferStats {
    pub uploaded_bytes: i64,
    pub downloaded_bytes: i64,
    pub files_added: i64,
    pub session_count: i64,
    pub seconds_active: i64,
}

pub fn blocklist_update() -> Request {
    Request::new(METHOD_BLOCKLIST_UPDATE)
}

/// The result of blocklist_update
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct BlocklistUpdate {
    pub blocklist_size: i64,
}

/// A port test over IPv4 or IPv6, or whatever the OS picks
pub fn port_test(ip_protocol: Option<&str>) -> Request {
    let mut req = Request::new(METHOD_PORT_TEST);
    if let Some(ip) = ip_protocol {
        req.set(PARAM_IP_PROTOCOL, ip);
    }
    req
}

/// The result of port_test
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PortTest {
    pub port_is_open: bool,
}

/// The session, the fields the application uses
pub fn session_get() -> Request {
    Request::new(METHOD_SESSION_GET).with(PARAM_FIELDS, session_fields())
}

pub fn session_set() -> Request {
    Request::new(METHOD_SESSION_SET)
}

/// The free space of a directory on the server
pub fn free_space(path: &str) -> Request {
    Request::new(METHOD_FREE_SPACE).with(PARAM_PATH, path)
}

/// The free space of a free_space result, -1 if it's unknown
pub fn free_space_bytes(result: &Obj) -> i64 {
    #[derive(Default, Deserialize)]
    #[serde(default)]
    struct FreeSpace {
        size_bytes: Option<i64>,
    }
    parse::<FreeSpace>(result).size_bytes.unwrap_or(-1)
}

pub fn torrent_set_location(ids: &[i64], location: &str, move_data: bool) -> Request {
    request(METHOD_TORRENT_SET_LOCATION, Some(ids))
        .with(FIELD_MOVE, move_data)
        .with(FIELD_LOCATION, location)
}

pub fn torrent_rename_path(ids: &[i64], path: &str, name: &str) -> Request {
    request(METHOD_TORRENT_RENAME_PATH, Some(ids)).with(FIELD_PATH, path).with(FIELD_NAME, name)
}

pub fn torrent_start(ids: Option<&[i64]>) -> Request {
    request(METHOD_TORRENT_START, ids)
}

pub fn torrent_pause(ids: Option<&[i64]>) -> Request {
    request(METHOD_TORRENT_STOP, ids)
}

pub fn torrent_reannounce(ids: &[i64]) -> Request {
    request(METHOD_TORRENT_REANNOUNCE, Some(ids))
}

pub fn torrent_verify(ids: &[i64]) -> Request {
    request(METHOD_TORRENT_VERIFY, Some(ids))
}

pub fn torrent_queue_move_up(ids: &[i64]) -> Request {
    request(METHOD_QUEUE_MOVE_UP, Some(ids))
}

pub fn torrent_queue_move_down(ids: &[i64]) -> Request {
    request(METHOD_QUEUE_MOVE_DOWN, Some(ids))
}

pub fn torrent_queue_move_top(ids: &[i64]) -> Request {
    request(METHOD_QUEUE_MOVE_TOP, Some(ids))
}

pub fn torrent_queue_move_bottom(ids: &[i64]) -> Request {
    request(METHOD_QUEUE_MOVE_BOTTOM, Some(ids))
}

pub fn torrent_start_now(ids: &[i64]) -> Request {
    request(METHOD_TORRENT_START_NOW, Some(ids))
}

pub fn torrent_set(ids: &[i64]) -> Request {
    request(METHOD_TORRENT_SET, Some(ids))
}

pub fn torrent_remove(ids: &[i64], remove_data: bool) -> Request {
    request(METHOD_TORRENT_REMOVE, Some(ids)).with(PARAM_DELETE_LOCAL_DATA, remove_data)
}

/// A torrent_get, in the table format: arrays of values rather than
/// objects, which repeat the keys for every torrent. rpc turns them back
/// into objects.
pub fn torrent_get(torrents: &Torrents) -> Request {
    let mut req = Request::new(METHOD_TORRENT_GET)
        .with(PARAM_FIELDS, torrent_fields())
        .with(FIELD_FORMAT, FORMAT_TABLE);
    torrents.set_on(&mut req);
    req
}

/// torrent_add for a URL or magnet link
pub fn torrent_add_url(url: &str, paused: bool) -> Request {
    Request::new(METHOD_TORRENT_ADD).with(PARAM_FILENAME, url).with(PARAM_PAUSED, paused)
}

/// torrent_add for the contents of a .torrent file (sent base64 encoded)
pub fn torrent_add_metainfo(metainfo: &[u8], paused: bool) -> Request {
    use base64::Engine;
    let metainfo = base64::engine::general_purpose::STANDARD.encode(metainfo);
    Request::new(METHOD_TORRENT_ADD).with(PARAM_METAINFO, metainfo).with(PARAM_PAUSED, paused)
}

/// The torrent of a torrent_add result
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AddedTorrent {
    pub id: i64,
    pub name: String,
}

/// The torrent of a torrent_add result, and whether it was there already
pub fn torrent_added(result: &Obj) -> Option<(AddedTorrent, bool)> {
    let torrent = |key| {
        get_obj(result, key).and_then(|t| serde_json::from_value(Value::Object(t.clone())).ok())
    };
    torrent(FIELD_TORRENT_ADDED)
        .map(|t| (t, false))
        .or_else(|| torrent(FIELD_TORRENT_DUPLICATE).map(|t| (t, true)))
}

/// Adds a file index to the array params[key], creating it if needed
pub fn add_file_id_to_array(req: &mut Request, key: &str, index: i64) {
    let entry = req.params_mut().entry(key.to_string()).or_insert_with(|| json!([]));
    if let Some(a) = entry.as_array_mut() {
        a.push(json!(index));
    }
}

/// The tracker_list of trackers: (tier, announce URL)
pub fn tracker_list(trackers: &[(i64, String)]) -> String {
    let mut sorted: Vec<&(i64, String)> = trackers.iter().collect();
    sorted.sort_by_key(|(tier, _)| *tier);
    let mut list = String::new();
    let mut last_tier = None;
    for (tier, url) in sorted {
        if last_tier.is_some() && last_tier != Some(*tier) {
            list.push('\n');
        }
        list.push_str(url);
        list.push('\n');
        last_tier = Some(*tier);
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_lists() {
        let trackers =
            [(1, "http://b".to_string()), (0, "http://a".into()), (1, "http://c".into())];
        assert_eq!(tracker_list(&trackers), "http://a\n\nhttp://b\nhttp://c\n");
        assert_eq!(tracker_list(&[]), "");
    }

    #[test]
    fn torrents_of_requests() {
        assert_eq!(Torrents::of(&torrent_verify(&[3, 4])), Torrents::Ids(vec![3, 4]));
        assert_eq!(Torrents::of(&torrent_start(None)), Torrents::All);
        assert_eq!(Torrents::of(&torrent_get(&Torrents::RecentlyActive)), Torrents::RecentlyActive);
        let get = torrent_get(&Torrents::Ids(vec![1]));
        assert_eq!(get.param(PARAM_IDS), Some(&json!([1])));
        assert_eq!(get.param(FIELD_FORMAT), Some(&json!(FORMAT_TABLE)));
    }
}
