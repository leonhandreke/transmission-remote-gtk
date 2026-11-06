//! The Transmission RPC protocol (4.1 and later, see docs/rpc-spec.md of
//! Transmission): the method names and keys, the requests, and getters for
//! the fields of torrents, trackers, peers, files and the session
//! (protocol-constants.h, requests.c, torrent.c, session-get.c and json.c
//! in the C version). The wire format is in rpc.

// the protocol's constants, not all of which are used
#![allow(dead_code)]

use serde_json::{Value, json};

use crate::i18n::tr;
use crate::prefs::{value_as_bool, value_as_f64, value_as_i64};

pub use crate::rpc::{Obj, Request};

pub const FIELD_ID: &str = "id";

pub const FIELD_RECENTLY_ACTIVE: &str = "recently_active";
pub const FIELD_TORRENTS: &str = "torrents";
pub const FIELD_REMOVED: &str = "removed";
pub const FIELD_LEFT_UNTIL_DONE: &str = "left_until_done";
pub const FIELD_TOTAL_SIZE: &str = "total_size";
pub const FIELD_DONE_DATE: &str = "done_date";
pub const FIELD_ADDED_DATE: &str = "added_date";
pub const FIELD_DATE_CREATED: &str = "date_created";
pub const FIELD_TRACKER_STATS: &str = "tracker_stats";
pub const FIELD_DOWNLOAD_DIR: &str = "download_dir";
pub const FIELD_HASH_STRING: &str = "hash_string";
pub const FIELD_NAME: &str = "name";
pub const FIELD_PATH: &str = "path";
pub const FIELD_SIZEWHENDONE: &str = "size_when_done";
pub const FIELD_STATUS: &str = "status";
pub const FIELD_MOVE: &str = "move";
pub const FIELD_CREATOR: &str = "creator";
pub const FIELD_LOCATION: &str = "location";
pub const FIELD_RATEDOWNLOAD: &str = "rate_download";
pub const FIELD_RATEUPLOAD: &str = "rate_upload";
pub const FIELD_ETA: &str = "eta";
pub const FIELD_UPLOADEDEVER: &str = "uploaded_ever";
pub const FIELD_DOWNLOADEDEVER: &str = "downloaded_ever";
pub const FIELD_CORRUPTEVER: &str = "corrupt_ever";
pub const FIELD_HAVEVALID: &str = "have_valid";
pub const FIELD_HAVEUNCHECKED: &str = "have_unchecked";
pub const FIELD_PERCENTDONE: &str = "percent_done";
pub const FIELD_PEERS: &str = "peers";
pub const FIELD_PEERSFROM: &str = "peers_from";
pub const FIELD_FILES: &str = "files";
pub const FIELD_WANTED: &str = "wanted";
pub const FIELD_WEB_SEEDS_SENDING_TO_US: &str = "webseeds_sending_to_us";
pub const FIELD_PRIORITIES: &str = "priorities";
pub const FIELD_COMMENT: &str = "comment";
pub const FIELD_ISFINISHED: &str = "is_finished";
pub const FIELD_ISPRIVATE: &str = "is_private";
pub const FIELD_MAGNETLINK: &str = "magnet_link";
pub const FIELD_ERROR: &str = "error";
pub const FIELD_ERROR_STRING: &str = "error_string";
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
pub const FIELD_PEERS_SENDING_TO_US: &str = "peers_sending_to_us";
pub const FIELD_PEERS_GETTING_FROM_US: &str = "peers_getting_from_us";
pub const FIELD_PEERS_CONNECTED: &str = "peers_connected";
pub const FIELD_QUEUE_POSITION: &str = "queue_position";
pub const FIELD_ACTIVITY_DATE: &str = "activity_date";
pub const FIELD_METADATAPERCENTCOMPLETE: &str = "metadata_percent_complete";
pub const FIELD_FILES_WANTED: &str = "files_wanted";
pub const FIELD_FILES_UNWANTED: &str = "files_unwanted";
pub const FIELD_FILES_PRIORITY_HIGH: &str = "priority_high";
pub const FIELD_FILES_PRIORITY_NORMAL: &str = "priority_normal";
pub const FIELD_FILES_PRIORITY_LOW: &str = "priority_low";
pub const FIELD_SEQUENTIAL_DOWNLOAD: &str = "sequential_download";
pub const FIELD_SEQUENTIAL_DOWNLOAD_FROM_PIECE: &str = "sequential_download_from_piece";
pub const FIELD_PIECE_COUNT: &str = "piece_count";
/// The announce URLs, one per line, with an empty line between tiers
pub const FIELD_TRACKER_LIST: &str = "tracker_list";
pub const FIELD_TORRENT_ADDED: &str = "torrent_added";
pub const FIELD_TORRENT_DUPLICATE: &str = "torrent_duplicate";
pub const FIELD_FORMAT: &str = "format";
pub const FORMAT_TABLE: &str = "table";

pub const FIELD_TIER: &str = "tier";
pub const FIELD_ANNOUNCE: &str = "announce";
pub const FIELD_SCRAPE: &str = "scrape";
pub const FIELD_LAST_ANNOUNCE_PEER_COUNT: &str = "last_announce_peer_count";
pub const FIELD_LAST_ANNOUNCE_TIME: &str = "last_announce_time";
pub const FIELD_LAST_SCRAPE_TIME: &str = "last_scrape_time";
pub const FIELD_SEEDERCOUNT: &str = "seeder_count";
pub const FIELD_LEECHERCOUNT: &str = "leecher_count";
pub const FIELD_DOWNLOADCOUNT: &str = "download_count";
pub const FIELD_HOST: &str = "host";
pub const FIELD_LAST_ANNOUNCE_RESULT: &str = "last_announce_result";
pub const FIELD_RECHECK_PROGRESS: &str = "recheck_progress";
pub const FIELD_DOWNLOADER_COUNT: &str = "downloader_count";

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

/* port_test, free_space and blocklist_update results */
pub const FIELD_PORT_IS_OPEN: &str = "port_is_open";
pub const FIELD_SIZE_BYTES: &str = "size_bytes";

/* session_stats */
pub const STATS_CURRENT: &str = "current_stats";
pub const STATS_CUMULATIVE: &str = "cumulative_stats";
pub const STATS_UPLOADED_BYTES: &str = "uploaded_bytes";
pub const STATS_DOWNLOADED_BYTES: &str = "downloaded_bytes";
pub const STATS_FILES_ADDED: &str = "files_added";
pub const STATS_SESSION_COUNT: &str = "session_count";
pub const STATS_SECONDS_ACTIVE: &str = "seconds_active";

pub const TPEER_ADDRESS: &str = "address";
pub const TPEER_CLIENT_NAME: &str = "client_name";
pub const TPEER_PROGRESS: &str = "progress";
pub const TPEER_RATE_TO_CLIENT: &str = "rate_to_client";
pub const TPEER_RATE_TO_PEER: &str = "rate_to_peer";
pub const TPEER_IS_ENCRYPTED: &str = "is_encrypted";
pub const TPEER_IS_DOWNLOADING_FROM: &str = "is_downloading_from";
pub const TPEER_IS_UPLOADING_TO: &str = "is_uploading_to";
pub const TPEER_FLAGSTR: &str = "flag_str";

pub const TPEERFROM_FROMPEX: &str = "from_pex";
pub const TPEERFROM_FROMDHT: &str = "from_dht";
pub const TPEERFROM_FROMTRACKERS: &str = "from_tracker";
pub const TPEERFROM_FROMLTEP: &str = "from_ltep";
pub const TPEERFROM_FROMRESUME: &str = "from_cache";
pub const TPEERFROM_FROMINCOMING: &str = "from_incoming";
pub const TPEERFROM_FROMLPD: &str = "from_lpd";

pub const TFILE_LENGTH: &str = "length";
pub const TFILE_BYTES_COMPLETED: &str = "bytes_completed";
pub const TFILE_NAME: &str = "name";

pub const TR_STATUS_STOPPED: i64 = 0;
pub const TR_STATUS_CHECK_WAIT: i64 = 1;
pub const TR_STATUS_CHECK: i64 = 2;
pub const TR_STATUS_DOWNLOAD_WAIT: i64 = 3;
pub const TR_STATUS_DOWNLOAD: i64 = 4;
pub const TR_STATUS_SEED_WAIT: i64 = 5;
pub const TR_STATUS_SEED: i64 = 6;

/// Not in the protocol, just used in the UI
pub const TR_PRI_UNSET: i64 = -3;
pub const TR_PRI_MIXED: i64 = -2;
pub const TR_PRI_LOW: i64 = -1;
pub const TR_PRI_NORMAL: i64 = 0;
pub const TR_PRI_HIGH: i64 = 1;

pub const TORRENT_FLAG_ERROR: u32 = 1 << 0;
pub const TORRENT_FLAG_COMPLETE: u32 = 1 << 1;
pub const TORRENT_FLAG_INCOMPLETE: u32 = 1 << 2;
pub const TORRENT_FLAG_SEEDING: u32 = 1 << 3;
pub const TORRENT_FLAG_SEEDING_WAIT: u32 = 1 << 4;
pub const TORRENT_FLAG_CHECKING: u32 = 1 << 5;
pub const TORRENT_FLAG_WAITING_CHECK: u32 = 1 << 6;
pub const TORRENT_FLAG_CHECKING_ANY: u32 = 1 << 7;
pub const TORRENT_FLAG_DOWNLOADING: u32 = 1 << 8;
pub const TORRENT_FLAG_DOWNLOADING_WAIT: u32 = 1 << 9;
pub const TORRENT_FLAG_PAUSED: u32 = 1 << 10;
pub const TORRENT_FLAG_QUEUED: u32 = 1 << 11;
pub const TORRENT_FLAG_ACTIVE: u32 = 1 << 12;
pub const TORRENT_FLAG_DOWNLOADING_METADATA: u32 = 1 << 13;

pub const TORRENT_ADD_FLAG_PAUSED: u32 = 1 << 0;
pub const TORRENT_ADD_FLAG_DELETE: u32 = 1 << 1;

pub const TORRENT_GET_MODE_FIRST: i32 = 0;
pub const TORRENT_GET_MODE_ACTIVE: i32 = 1;
pub const TORRENT_GET_MODE_INTERACTION: i32 = 2;
pub const TORRENT_GET_MODE_UPDATE: i32 = 3;

/* session_get */
/// Not asked for (it's deprecated), but filled in by the client with the
/// result of free_space for download_dir
pub const SGET_DOWNLOAD_DIR_FREE_SPACE: &str = "download_dir_free_space";
pub const SGET_BLOCKLIST_ENABLED: &str = "blocklist_enabled";
pub const SGET_BLOCKLIST_URL: &str = "blocklist_url";
pub const SGET_BLOCKLIST_SIZE: &str = "blocklist_size";
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
pub const SGET_RPC_VERSION_SEMVER: &str = "rpc_version_semver";
pub const SGET_SEED_RATIO_LIMIT: &str = "seed_ratio_limit";
pub const SGET_SEED_RATIO_LIMITED: &str = "seed_ratio_limited";
pub const SGET_SPEED_LIMIT_DOWN: &str = "speed_limit_down";
pub const SGET_SPEED_LIMIT_DOWN_ENABLED: &str = "speed_limit_down_enabled";
pub const SGET_SPEED_LIMIT_UP: &str = "speed_limit_up";
pub const SGET_SPEED_LIMIT_UP_ENABLED: &str = "speed_limit_up_enabled";
pub const SGET_VERSION: &str = "version";
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

/// The session fields asked for, all of the above
const SESSION_GET_FIELDS: &[&str] = &[
    SGET_BLOCKLIST_ENABLED,
    SGET_BLOCKLIST_URL,
    SGET_BLOCKLIST_SIZE,
    SGET_DHT_ENABLED,
    SGET_LPD_ENABLED,
    SGET_DOWNLOAD_DIR,
    SGET_INCOMPLETE_DIR,
    SGET_INCOMPLETE_DIR_ENABLED,
    SGET_ENCRYPTION,
    SGET_PEER_LIMIT_GLOBAL,
    SGET_PEER_LIMIT_PER_TORRENT,
    SGET_PEER_PORT,
    SGET_PEER_PORT_RANDOM_ON_START,
    SGET_PEX_ENABLED,
    SGET_PORT_FORWARDING_ENABLED,
    SGET_RPC_VERSION_SEMVER,
    SGET_SEED_RATIO_LIMIT,
    SGET_SEED_RATIO_LIMITED,
    SGET_SPEED_LIMIT_DOWN,
    SGET_SPEED_LIMIT_DOWN_ENABLED,
    SGET_SPEED_LIMIT_UP,
    SGET_SPEED_LIMIT_UP_ENABLED,
    SGET_VERSION,
    SGET_TRASH_ORIGINAL_TORRENT_FILES,
    SGET_START_ADDED_TORRENTS,
    SGET_RENAME_PARTIAL_FILES,
    SGET_CACHE_SIZE_MIB,
    SGET_SCRIPT_TORRENT_DONE_FILENAME,
    SGET_SCRIPT_TORRENT_DONE_ENABLED,
    SGET_DOWNLOAD_QUEUE_ENABLED,
    SGET_DOWNLOAD_QUEUE_SIZE,
    SGET_SEED_QUEUE_ENABLED,
    SGET_SEED_QUEUE_SIZE,
    SGET_QUEUE_STALLED_ENABLED,
    SGET_QUEUE_STALLED_MINUTES,
    SGET_ALT_SPEED_DOWN,
    SGET_ALT_SPEED_ENABLED,
    SGET_ALT_SPEED_TIME_BEGIN,
    SGET_ALT_SPEED_TIME_ENABLED,
    SGET_ALT_SPEED_TIME_END,
    SGET_ALT_SPEED_TIME_DAY,
    SGET_ALT_SPEED_UP,
    SGET_SEQUENTIAL_DOWNLOAD,
    SGET_PREFERRED_TRANSPORTS,
];

/* JSON getters, like json-glib's (0, false or None when missing) */

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

pub fn get_array<'a>(o: &'a Obj, key: &str) -> &'a [Value] {
    o.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

pub fn get_obj<'a>(o: &'a Obj, key: &str) -> Option<&'a Obj> {
    o.get(key).and_then(Value::as_object)
}

pub fn objects(array: &[Value]) -> impl Iterator<Item = &Obj> {
    array.iter().filter_map(Value::as_object)
}

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

pub fn session_stats() -> Request {
    Request::new(METHOD_SESSION_STATS)
}

pub fn blocklist_update() -> Request {
    Request::new(METHOD_BLOCKLIST_UPDATE)
}

/// A port test over IPv4 or IPv6, or whatever the OS picks
pub fn port_test(ip_protocol: Option<&str>) -> Request {
    let mut req = Request::new(METHOD_PORT_TEST);
    if let Some(ip) = ip_protocol {
        req.set(PARAM_IP_PROTOCOL, ip);
    }
    req
}

/// The session, the fields the application uses
pub fn session_get() -> Request {
    Request::new(METHOD_SESSION_GET).with(PARAM_FIELDS, SESSION_GET_FIELDS)
}

pub fn session_set() -> Request {
    Request::new(METHOD_SESSION_SET)
}

/// The free space of a directory on the server
pub fn free_space(path: &str) -> Request {
    Request::new(METHOD_FREE_SPACE).with(PARAM_PATH, path)
}

pub fn free_space_bytes(result: &Obj) -> i64 {
    result.get(FIELD_SIZE_BYTES).map(value_as_i64).unwrap_or(-1)
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

const TORRENT_GET_FIELDS: &[&str] = &[
    FIELD_ETA,
    FIELD_PEERS,
    FIELD_PEERSFROM,
    FIELD_FILES,
    FIELD_PEERS_SENDING_TO_US,
    FIELD_PEERS_GETTING_FROM_US,
    FIELD_WEB_SEEDS_SENDING_TO_US,
    FIELD_PEERS_CONNECTED,
    FIELD_HAVEVALID,
    FIELD_HAVEUNCHECKED,
    FIELD_RATEUPLOAD,
    FIELD_RATEDOWNLOAD,
    FIELD_STATUS,
    FIELD_ISFINISHED,
    FIELD_ISPRIVATE,
    FIELD_ADDED_DATE,
    FIELD_DOWNLOADEDEVER,
    FIELD_UPLOADEDEVER,
    FIELD_CORRUPTEVER,
    FIELD_SIZEWHENDONE,
    FIELD_QUEUE_POSITION,
    FIELD_ID,
    FIELD_NAME,
    FIELD_PERCENTDONE,
    FIELD_COMMENT,
    FIELD_TOTAL_SIZE,
    FIELD_METADATAPERCENTCOMPLETE,
    FIELD_LEFT_UNTIL_DONE,
    FIELD_TRACKER_STATS,
    FIELD_DATE_CREATED,
    FIELD_DOWNLOAD_DIR,
    FIELD_CREATOR,
    FIELD_HASH_STRING,
    FIELD_DONE_DATE,
    FIELD_HONORS_SESSION_LIMITS,
    FIELD_UPLOAD_LIMIT,
    FIELD_UPLOAD_LIMITED,
    FIELD_DOWNLOAD_LIMIT,
    FIELD_DOWNLOAD_LIMITED,
    FIELD_BANDWIDTH_PRIORITY,
    FIELD_SEED_RATIO_LIMIT,
    FIELD_SEED_RATIO_MODE,
    FIELD_PEER_LIMIT,
    FIELD_ACTIVITY_DATE,
    FIELD_MAGNETLINK,
    FIELD_ERROR,
    FIELD_ERROR_STRING,
    FIELD_WANTED,
    FIELD_PRIORITIES,
    FIELD_RECHECK_PROGRESS,
    FIELD_SEQUENTIAL_DOWNLOAD,
    FIELD_SEQUENTIAL_DOWNLOAD_FROM_PIECE,
    FIELD_PIECE_COUNT,
];

/// A torrent_get, in the table format: arrays of values rather than
/// objects, which repeat the keys for every torrent. rpc turns them back
/// into objects.
pub fn torrent_get(torrents: &Torrents) -> Request {
    let mut req = Request::new(METHOD_TORRENT_GET)
        .with(PARAM_FIELDS, TORRENT_GET_FIELDS)
        .with(FIELD_FORMAT, FORMAT_TABLE);
    torrents.set_on(&mut req);
    req
}

pub fn torrent_add_url(url: &str, paused: bool) -> Request {
    Request::new(METHOD_TORRENT_ADD).with(PARAM_FILENAME, url).with(PARAM_PAUSED, paused)
}

/// torrent_add for a file (sent base64 encoded), URL or magnet link. None
/// (with a message) if the file doesn't exist; an error if it can't be read.
pub fn torrent_add_from_file(target: &str, flags: u32) -> Result<Option<Request>, String> {
    let is_uri = crate::util::is_magnet(target) || crate::util::is_url(target);

    if !is_uri && !std::path::Path::new(target).is_file() {
        glib_message(&format!("file \"{target}\" does not exist."));
        return Ok(None);
    }

    let mut req = Request::new(METHOD_TORRENT_ADD);
    if is_uri {
        req.set(PARAM_FILENAME, target);
    } else {
        req.set(PARAM_METAINFO, crate::util::base64_encode_file(target)?);
    }
    req.set(PARAM_PAUSED, flags & TORRENT_ADD_FLAG_PAUSED != 0);

    if flags & TORRENT_ADD_FLAG_DELETE != 0 {
        let _ = std::fs::remove_file(target);
    }

    Ok(Some(req))
}

/// The torrent of a torrent_add result ({id, name, hash_string}), and
/// whether it was there already
pub fn torrent_added(result: &Obj) -> Option<(&Obj, bool)> {
    get_obj(result, FIELD_TORRENT_ADDED)
        .map(|t| (t, false))
        .or_else(|| get_obj(result, FIELD_TORRENT_DUPLICATE).map(|t| (t, true)))
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

fn glib_message(msg: &str) {
    gtk::glib::g_message!("trg", "{}", msg);
}

/* Torrents */

pub fn torrent_get_id(t: &Obj) -> i64 {
    get_i64(t, FIELD_ID)
}

pub fn torrent_get_name(t: &Obj) -> &str {
    get_str(t, FIELD_NAME).unwrap_or("")
}

pub fn torrent_get_files(t: &Obj) -> &[Value] {
    get_array(t, FIELD_FILES)
}

pub fn torrent_get_peers(t: &Obj) -> &[Value] {
    get_array(t, FIELD_PEERS)
}

pub fn torrent_get_wanted(t: &Obj) -> &[Value] {
    get_array(t, FIELD_WANTED)
}

pub fn torrent_get_priorities(t: &Obj) -> &[Value] {
    get_array(t, FIELD_PRIORITIES)
}

pub fn torrent_get_tracker_stats(t: &Obj) -> &[Value] {
    get_array(t, FIELD_TRACKER_STATS)
}

/// The ratio of a node holding an int or a double, times 100
pub fn json_double_to_progress(v: Option<&Value>) -> f64 {
    v.map(value_as_f64).unwrap_or(0.0) * 100.0
}

pub fn torrent_get_metadata_percent_complete(t: &Obj) -> f64 {
    match t.get(FIELD_METADATAPERCENTCOMPLETE) {
        Some(v) => json_double_to_progress(Some(v)),
        None => 100.0,
    }
}

pub fn torrent_get_percent_done(t: &Obj) -> f64 {
    json_double_to_progress(t.get(FIELD_PERCENTDONE))
}

pub fn torrent_get_recheck_progress(t: &Obj) -> f64 {
    json_double_to_progress(t.get(FIELD_RECHECK_PROGRESS))
}

pub fn torrent_get_is_finished(t: &Obj) -> bool {
    get_i64(t, FIELD_LEFT_UNTIL_DONE) <= 0
}

pub fn torrent_get_error(t: &Obj) -> i64 {
    get_i64(t, FIELD_ERROR)
}

pub fn torrent_get_queue_position(t: &Obj) -> i64 {
    if t.contains_key(FIELD_QUEUE_POSITION) { get_i64(t, FIELD_QUEUE_POSITION) } else { -1 }
}

pub fn torrent_get_flags(t: &Obj, status: i64, file_count: i64) -> u32 {
    let mut flags = 0;

    if file_count > 0 && torrent_get_is_finished(t) {
        flags |= TORRENT_FLAG_COMPLETE;
    } else {
        flags |= TORRENT_FLAG_INCOMPLETE;
    }

    match status {
        TR_STATUS_STOPPED => flags |= TORRENT_FLAG_PAUSED,
        TR_STATUS_CHECK_WAIT => flags |= TORRENT_FLAG_WAITING_CHECK | TORRENT_FLAG_CHECKING_ANY,
        TR_STATUS_CHECK => flags |= TORRENT_FLAG_CHECKING | TORRENT_FLAG_CHECKING_ANY,
        TR_STATUS_DOWNLOAD_WAIT => flags |= TORRENT_FLAG_DOWNLOADING_WAIT | TORRENT_FLAG_QUEUED,
        TR_STATUS_DOWNLOAD => {
            if flags & TORRENT_FLAG_COMPLETE == 0 {
                flags |= TORRENT_FLAG_DOWNLOADING;
            }
            if file_count <= 0 {
                flags |= TORRENT_FLAG_DOWNLOADING_METADATA;
            }
            flags |= TORRENT_FLAG_ACTIVE;
        }
        TR_STATUS_SEED_WAIT => flags |= TORRENT_FLAG_SEEDING_WAIT,
        TR_STATUS_SEED => {
            flags |= TORRENT_FLAG_SEEDING;
            if get_i64(t, FIELD_PEERS_GETTING_FROM_US) != 0 {
                flags |= TORRENT_FLAG_ACTIVE;
            }
        }
        _ => {}
    }

    if torrent_get_error(t) > 0 {
        flags |= TORRENT_FLAG_ERROR;
    }

    flags
}

pub fn torrent_get_status_icon(flags: u32) -> &'static str {
    if flags & TORRENT_FLAG_ERROR != 0 {
        "dialog-warning"
    } else if flags & TORRENT_FLAG_DOWNLOADING_METADATA != 0 {
        "edit-find"
    } else if flags & TORRENT_FLAG_DOWNLOADING != 0 {
        "go-down"
    } else if flags & TORRENT_FLAG_PAUSED != 0 {
        "media-playback-pause"
    } else if flags & TORRENT_FLAG_SEEDING != 0 {
        "go-up"
    } else if flags & TORRENT_FLAG_CHECKING != 0 {
        "view-refresh"
    } else if flags & TORRENT_FLAG_DOWNLOADING_WAIT != 0 {
        "media-seek-backward"
    } else if flags & TORRENT_FLAG_SEEDING_WAIT != 0 {
        "media-seek-forward"
    } else {
        "dialog-question"
    }
}

pub fn torrent_get_status_string(value: i64, flags: u32) -> String {
    let metadata = flags & TORRENT_FLAG_DOWNLOADING_METADATA != 0;
    match value {
        TR_STATUS_DOWNLOAD if metadata => tr("Metadata Downloading"),
        TR_STATUS_DOWNLOAD => tr("Downloading"),
        TR_STATUS_DOWNLOAD_WAIT => tr("Queued download"),
        TR_STATUS_CHECK_WAIT => tr("Waiting To Check"),
        TR_STATUS_CHECK => tr("Checking"),
        TR_STATUS_SEED_WAIT => tr("Queued seed"),
        TR_STATUS_SEED => tr("Seeding"),
        TR_STATUS_STOPPED => tr("Paused"),
        _ => tr("Unknown"),
    }
}

pub fn torrent_has_tracker(t: &Obj, rx: &crate::util::HostRegex, search: Option<&str>) -> bool {
    objects(torrent_get_tracker_stats(t)).any(|tracker| {
        let host = get_str(tracker, FIELD_ANNOUNCE).and_then(|a| rx.first(a));
        host.as_deref() == search
    })
}

pub fn get_torrents(args: &Obj) -> &[Value] {
    get_array(args, FIELD_TORRENTS)
}

pub fn get_torrents_removed(args: &Obj) -> Option<&[Value]> {
    args.get(FIELD_REMOVED).and_then(Value::as_array).map(Vec::as_slice)
}

pub fn torrent_get_full_path(t: &Obj) -> String {
    format!(
        "{}/{}",
        get_str(t, FIELD_DOWNLOAD_DIR).unwrap_or(""),
        get_str(t, FIELD_NAME).unwrap_or("")
    )
}

/// The directory containing the torrent's files: its download directory, or
/// the directory its (first) file is in below that.
pub fn torrent_get_full_dir(t: &Obj) -> String {
    let location = get_str(t, FIELD_DOWNLOAD_DIR).unwrap_or("");
    let name =
        objects(torrent_get_files(t)).next().and_then(|f| get_str(f, TFILE_NAME)).unwrap_or("");
    match name.find('/') {
        Some(i) => format!("{location}/{}", &name[..i]),
        None => location.to_string(),
    }
}

/* Peers */

pub fn peerfrom_get_lpd(pf: &Obj) -> i64 {
    if pf.contains_key(TPEERFROM_FROMLPD) { get_i64(pf, TPEERFROM_FROMLPD) } else { -1 }
}

/* Files */

pub fn file_get_progress(length: i64, completed: i64) -> f64 {
    if length > 0 { ((completed as f64 / length as f64) * 100.0).min(100.0) } else { 0.0 }
}

/* Session */

/// The version number of "2.94 (d8e60ee44f)" (0 if there's no session)
pub fn session_get_version(s: Option<&Obj>) -> f64 {
    let Some(v) = s.and_then(|s| get_str(s, SGET_VERSION)) else {
        return 0.0;
    };
    // the leading number, like g_ascii_strtod(): "4.1.4 (a6d907d9ea)" is 4.1
    let mut seen_dot = false;
    let num: String = v
        .trim()
        .chars()
        .take_while(|c| {
            if *c == '.' && !seen_dot {
                seen_dot = true;
                true
            } else {
                c.is_ascii_digit()
            }
        })
        .collect();
    num.trim_end_matches('.').parse().unwrap_or(0.0)
}

pub fn session_get_cache_size_mib(s: &Obj) -> i64 {
    if s.contains_key(SGET_CACHE_SIZE_MIB) { get_i64(s, SGET_CACHE_SIZE_MIB) } else { -1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        let s = |v: &str| json!({ SGET_VERSION: v }).as_object().unwrap().clone();
        assert_eq!(session_get_version(Some(&s("4.1.4 (a6d907d9ea)"))), 4.1);
        assert_eq!(session_get_version(Some(&s("2.94"))), 2.94);
        assert_eq!(session_get_version(None), 0.0);
    }

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
