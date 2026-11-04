//! The Transmission RPC protocol: constants, the requests (JSON objects
//! ready for dispatch) and getters for the fields of torrents, trackers,
//! peers, files and the session (protocol-constants.h, requests.c,
//! torrent.c, session-get.c and json.c in the C version).

// the protocol's constants, not all of which are used
#![allow(dead_code)]

use serde_json::{Map, Value, json};

use crate::i18n::tr;
use crate::prefs::{value_as_bool, value_as_f64, value_as_i64};

pub type Obj = Map<String, Value>;

pub const PARAM_METHOD: &str = "method";
pub const FIELD_ID: &str = "id";
pub const FIELD_RESULT: &str = "result";
pub const FIELD_SUCCESS: &str = "success";

pub const FIELD_RECENTLY_ACTIVE: &str = "recently-active";
pub const FIELD_TORRENTS: &str = "torrents";
pub const FIELD_REMOVED: &str = "removed";
pub const FIELD_ANNOUNCE_URL: &str = "announceUrl";
pub const FIELD_LEFT_UNTIL_DONE: &str = "leftUntilDone";
pub const FIELD_TOTAL_SIZE: &str = "totalSize";
pub const FIELD_DONE_DATE: &str = "doneDate";
pub const FIELD_ADDED_DATE: &str = "addedDate";
pub const FIELD_DATE_CREATED: &str = "dateCreated";
pub const FIELD_TRACKER_STATS: &str = "trackerStats";
pub const FIELD_DOWNLOAD_DIR: &str = "downloadDir";
pub const FIELD_HASH_STRING: &str = "hashString";
pub const FIELD_NAME: &str = "name";
pub const FIELD_PATH: &str = "path";
pub const FIELD_SIZEWHENDONE: &str = "sizeWhenDone";
pub const FIELD_STATUS: &str = "status";
pub const FIELD_MOVE: &str = "move";
pub const FIELD_CREATOR: &str = "creator";
pub const FIELD_LOCATION: &str = "location";
pub const FIELD_RATEDOWNLOAD: &str = "rateDownload";
pub const FIELD_RATEUPLOAD: &str = "rateUpload";
pub const FIELD_ETA: &str = "eta";
pub const FIELD_UPLOADEDEVER: &str = "uploadedEver";
pub const FIELD_DOWNLOADEDEVER: &str = "downloadedEver";
pub const FIELD_CORRUPTEVER: &str = "corruptEver";
pub const FIELD_HAVEVALID: &str = "haveValid";
pub const FIELD_HAVEUNCHECKED: &str = "haveUnchecked";
pub const FIELD_PERCENTDONE: &str = "percentDone";
pub const FIELD_PEERS: &str = "peers";
pub const FIELD_PEERSFROM: &str = "peersFrom";
pub const FIELD_FILES: &str = "files";
pub const FIELD_WANTED: &str = "wanted";
pub const FIELD_WEB_SEEDS_SENDING_TO_US: &str = "webseedsSendingToUs";
pub const FIELD_PRIORITIES: &str = "priorities";
pub const FIELD_COMMENT: &str = "comment";
pub const FIELD_ISFINISHED: &str = "isFinished";
pub const FIELD_ISPRIVATE: &str = "isPrivate";
pub const FIELD_MAGNETLINK: &str = "magnetLink";
pub const FIELD_ERROR: &str = "error";
pub const FIELD_ERROR_STRING: &str = "errorString";
pub const FIELD_BANDWIDTH_PRIORITY: &str = "bandwidthPriority";
pub const FIELD_UPLOAD_LIMIT: &str = "uploadLimit";
pub const FIELD_UPLOAD_LIMITED: &str = "uploadLimited";
pub const FIELD_DOWNLOAD_LIMIT: &str = "downloadLimit";
pub const FIELD_DOWNLOAD_LIMITED: &str = "downloadLimited";
pub const FIELD_HONORS_SESSION_LIMITS: &str = "honorsSessionLimits";
pub const FIELD_SEED_RATIO_MODE: &str = "seedRatioMode";
pub const FIELD_SEED_RATIO_LIMIT: &str = "seedRatioLimit";
pub const FIELD_PEER_LIMIT: &str = "peer-limit";
pub const FIELD_FILE_DOWNLOAD_DIR: &str = "download-dir";
pub const FIELD_PEERS_SENDING_TO_US: &str = "peersSendingToUs";
pub const FIELD_PEERS_GETTING_FROM_US: &str = "peersGettingFromUs";
pub const FIELD_PEERS_CONNECTED: &str = "peersConnected";
pub const FIELD_QUEUE_POSITION: &str = "queuePosition";
pub const FIELD_ACTIVITY_DATE: &str = "activityDate";
pub const FIELD_METADATAPERCENTCOMPLETE: &str = "metadataPercentComplete";
pub const FIELD_FILES_WANTED: &str = "files-wanted";
pub const FIELD_FILES_UNWANTED: &str = "files-unwanted";
pub const FIELD_FILES_PRIORITY_HIGH: &str = "priority-high";
pub const FIELD_FILES_PRIORITY_NORMAL: &str = "priority-normal";
pub const FIELD_FILES_PRIORITY_LOW: &str = "priority-low";

pub const FIELD_TIER: &str = "tier";
pub const FIELD_ANNOUNCE: &str = "announce";
pub const FIELD_SCRAPE: &str = "scrape";
pub const FIELD_LAST_ANNOUNCE_PEER_COUNT: &str = "lastAnnouncePeerCount";
pub const FIELD_LAST_ANNOUNCE_TIME: &str = "lastAnnounceTime";
pub const FIELD_LAST_SCRAPE_TIME: &str = "lastScrapeTime";
pub const FIELD_SEEDERCOUNT: &str = "seederCount";
pub const FIELD_LEECHERCOUNT: &str = "leecherCount";
pub const FIELD_DOWNLOADCOUNT: &str = "downloadCount";
pub const FIELD_HOST: &str = "host";
pub const FIELD_LAST_ANNOUNCE_RESULT: &str = "lastAnnounceResult";
pub const FIELD_RECHECK_PROGRESS: &str = "recheckProgress";

pub const METHOD_TORRENT_START: &str = "torrent-start";
pub const METHOD_SESSION_GET: &str = "session-get";
pub const METHOD_SESSION_SET: &str = "session-set";
pub const METHOD_TORRENT_GET: &str = "torrent-get";
pub const METHOD_TORRENT_SET: &str = "torrent-set";
pub const METHOD_TORRENT_SET_LOCATION: &str = "torrent-set-location";
pub const METHOD_TORRENT_RENAME_PATH: &str = "torrent-rename-path";
pub const METHOD_TORRENT_STOP: &str = "torrent-stop";
pub const METHOD_TORRENT_VERIFY: &str = "torrent-verify";
pub const METHOD_TORRENT_REMOVE: &str = "torrent-remove";
pub const METHOD_TORRENT_ADD: &str = "torrent-add";
pub const METHOD_TORRENT_REANNOUNCE: &str = "torrent-reannounce";
pub const METHOD_PORT_TEST: &str = "port-test";
pub const METHOD_BLOCKLIST_UPDATE: &str = "blocklist-update";
pub const METHOD_SESSION_STATS: &str = "session-stats";
pub const METHOD_QUEUE_MOVE_TOP: &str = "queue-move-top";
pub const METHOD_QUEUE_MOVE_UP: &str = "queue-move-up";
pub const METHOD_QUEUE_MOVE_BOTTOM: &str = "queue-move-bottom";
pub const METHOD_QUEUE_MOVE_DOWN: &str = "queue-move-down";
pub const METHOD_TORRENT_START_NOW: &str = "torrent-start-now";

pub const PARAM_IDS: &str = "ids";
pub const PARAM_DELETE_LOCAL_DATA: &str = "delete-local-data";
pub const PARAM_ARGUMENTS: &str = "arguments";
pub const PARAM_FIELDS: &str = "fields";
pub const PARAM_METAINFO: &str = "metainfo";
pub const PARAM_FILENAME: &str = "filename";
pub const PARAM_PAUSED: &str = "paused";
pub const PARAM_TAG: &str = "tag";

pub const TPEER_ADDRESS: &str = "address";
pub const TPEER_CLIENT_NAME: &str = "clientName";
pub const TPEER_PROGRESS: &str = "progress";
pub const TPEER_RATE_TO_CLIENT: &str = "rateToClient";
pub const TPEER_RATE_TO_PEER: &str = "rateToPeer";
pub const TPEER_IS_ENCRYPTED: &str = "isEncrypted";
pub const TPEER_IS_DOWNLOADING_FROM: &str = "isDownloadingFrom";
pub const TPEER_IS_UPLOADING_TO: &str = "isUploadingTo";
pub const TPEER_FLAGSTR: &str = "flagStr";

pub const TPEERFROM_FROMPEX: &str = "fromPex";
pub const TPEERFROM_FROMDHT: &str = "fromDht";
pub const TPEERFROM_FROMTRACKERS: &str = "fromTracker";
pub const TPEERFROM_FROMLTEP: &str = "fromLtep";
pub const TPEERFROM_FROMRESUME: &str = "fromCache";
pub const TPEERFROM_FROMINCOMING: &str = "fromIncoming";
pub const TPEERFROM_FROMLPD: &str = "fromLpd";

pub const TFILE_LENGTH: &str = "length";
pub const TFILE_BYTES_COMPLETED: &str = "bytesCompleted";
pub const TFILE_NAME: &str = "name";

/// The rpc-version from which the status field of torrent-get changed
pub const NEW_STATUS_RPC_VERSION: i64 = 14;

pub const OLD_STATUS_WAITING_TO_CHECK: i64 = 1;
pub const OLD_STATUS_CHECKING: i64 = 2;
pub const OLD_STATUS_DOWNLOADING: i64 = 4;
pub const OLD_STATUS_SEEDING: i64 = 8;
pub const OLD_STATUS_PAUSED: i64 = 16;

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

pub const TORRENT_GET_TAG_MODE_FULL: i64 = -1;
pub const TORRENT_GET_TAG_MODE_UPDATE: i64 = -2;

/* session-get */
pub const SGET_DOWNLOAD_DIR_FREE_SPACE: &str = "download-dir-free-space";
pub const SGET_BLOCKLIST_ENABLED: &str = "blocklist-enabled";
pub const SGET_BLOCKLIST_URL: &str = "blocklist-url";
pub const SGET_BLOCKLIST_SIZE: &str = "blocklist-size";
pub const SGET_DHT_ENABLED: &str = "dht-enabled";
pub const SGET_LPD_ENABLED: &str = "lpd-enabled";
pub const SGET_DOWNLOAD_DIR: &str = "download-dir";
pub const SGET_INCOMPLETE_DIR: &str = "incomplete-dir";
pub const SGET_INCOMPLETE_DIR_ENABLED: &str = "incomplete-dir-enabled";
pub const SGET_ENCRYPTION: &str = "encryption";
pub const SGET_PEER_LIMIT_GLOBAL: &str = "peer-limit-global";
pub const SGET_PEER_LIMIT_PER_TORRENT: &str = "peer-limit-per-torrent";
pub const SGET_PEER_PORT: &str = "peer-port";
pub const SGET_PEER_PORT_RANDOM_ON_START: &str = "peer-port-random-on-start";
pub const SGET_PEX_ENABLED: &str = "pex-enabled";
pub const SGET_PORT_FORWARDING_ENABLED: &str = "port-forwarding-enabled";
pub const SGET_RPC_VERSION: &str = "rpc-version";
pub const SGET_RPC_VERSION_MINIMUM: &str = "rpc-version-minimum";
pub const SGET_SEED_RATIO_LIMIT: &str = "seedRatioLimit";
pub const SGET_SEED_RATIO_LIMITED: &str = "seedRatioLimited";
pub const SGET_SPEED_LIMIT_DOWN: &str = "speed-limit-down";
pub const SGET_SPEED_LIMIT_DOWN_ENABLED: &str = "speed-limit-down-enabled";
pub const SGET_SPEED_LIMIT_UP: &str = "speed-limit-up";
pub const SGET_SPEED_LIMIT_UP_ENABLED: &str = "speed-limit-up-enabled";
pub const SGET_VERSION: &str = "version";
pub const SGET_TRASH_ORIGINAL_TORRENT_FILES: &str = "trash-original-torrent-files";
pub const SGET_START_ADDED_TORRENTS: &str = "start-added-torrents";
pub const SGET_RENAME_PARTIAL_FILES: &str = "rename-partial-files";
pub const SGET_CACHE_SIZE_MB: &str = "cache-size-mb";
pub const SGET_SCRIPT_TORRENT_DONE_FILENAME: &str = "script-torrent-done-filename";
pub const SGET_SCRIPT_TORRENT_DONE_ENABLED: &str = "script-torrent-done-enabled";
pub const SGET_DOWNLOAD_QUEUE_ENABLED: &str = "download-queue-enabled";
pub const SGET_DOWNLOAD_QUEUE_SIZE: &str = "download-queue-size";
pub const SGET_SEED_QUEUE_ENABLED: &str = "seed-queue-enabled";
pub const SGET_SEED_QUEUE_SIZE: &str = "seed-queue-size";
pub const SGET_QUEUE_STALLED_ENABLED: &str = "queue-stalled-enabled";
pub const SGET_QUEUE_STALLED_MINUTES: &str = "queue-stalled-minutes";
pub const SGET_ALT_SPEED_DOWN: &str = "alt-speed-down";
pub const SGET_ALT_SPEED_ENABLED: &str = "alt-speed-enabled";
pub const SGET_ALT_SPEED_TIME_BEGIN: &str = "alt-speed-time-begin";
pub const SGET_ALT_SPEED_TIME_ENABLED: &str = "alt-speed-time-enabled";
pub const SGET_ALT_SPEED_TIME_END: &str = "alt-speed-time-end";
pub const SGET_ALT_SPEED_TIME_DAY: &str = "alt-speed-time-day";
pub const SGET_ALT_SPEED_UP: &str = "alt-speed-up";

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

pub type Request = Value;

fn base_request(method: &str) -> Request {
    json!({ PARAM_METHOD: method, PARAM_ARGUMENTS: {} })
}

pub fn args_mut(req: &mut Request) -> &mut Obj {
    req.get_mut(PARAM_ARGUMENTS).and_then(Value::as_object_mut).unwrap()
}

pub fn arguments(response: &Obj) -> Option<&Obj> {
    get_obj(response, PARAM_ARGUMENTS)
}

pub fn request_set_tag(req: &mut Request, tag: i64) {
    req.as_object_mut().unwrap().insert(PARAM_TAG.into(), json!(tag));
}

fn request_set_tag_from_ids(req: &mut Request, ids: &[i64]) {
    let tag = if ids.len() == 1 { ids[0] } else { TORRENT_GET_TAG_MODE_FULL };
    request_set_tag(req, tag);
}

pub fn generic_request(method: &str, ids: Option<&[i64]>) -> Request {
    let mut req = base_request(method);
    if let Some(ids) = ids {
        args_mut(&mut req).insert(PARAM_IDS.into(), json!(ids));
        request_set_tag_from_ids(&mut req, ids);
    }
    req
}

pub fn session_stats() -> Request {
    base_request(METHOD_SESSION_STATS)
}

pub fn blocklist_update() -> Request {
    base_request(METHOD_BLOCKLIST_UPDATE)
}

pub fn port_test() -> Request {
    base_request(METHOD_PORT_TEST)
}

pub fn session_get() -> Request {
    base_request(METHOD_SESSION_GET)
}

pub fn session_set() -> Request {
    generic_request(METHOD_SESSION_SET, None)
}

pub fn torrent_set_location(ids: &[i64], location: &str, move_data: bool) -> Request {
    let mut req = generic_request(METHOD_TORRENT_SET_LOCATION, Some(ids));
    let args = args_mut(&mut req);
    args.insert(FIELD_MOVE.into(), json!(move_data));
    args.insert(FIELD_LOCATION.into(), json!(location));
    req
}

pub fn torrent_rename_path(ids: &[i64], path: &str, name: &str) -> Request {
    let mut req = generic_request(METHOD_TORRENT_RENAME_PATH, Some(ids));
    let args = args_mut(&mut req);
    args.insert(FIELD_PATH.into(), json!(path));
    args.insert(FIELD_NAME.into(), json!(name));
    req
}

pub fn torrent_start(ids: Option<&[i64]>) -> Request {
    generic_request(METHOD_TORRENT_START, ids)
}

pub fn torrent_pause(ids: Option<&[i64]>) -> Request {
    generic_request(METHOD_TORRENT_STOP, ids)
}

pub fn torrent_reannounce(ids: &[i64]) -> Request {
    generic_request(METHOD_TORRENT_REANNOUNCE, Some(ids))
}

pub fn torrent_verify(ids: &[i64]) -> Request {
    generic_request(METHOD_TORRENT_VERIFY, Some(ids))
}

pub fn torrent_queue_move_up(ids: &[i64]) -> Request {
    generic_request(METHOD_QUEUE_MOVE_UP, Some(ids))
}

pub fn torrent_queue_move_down(ids: &[i64]) -> Request {
    generic_request(METHOD_QUEUE_MOVE_DOWN, Some(ids))
}

pub fn torrent_queue_move_top(ids: &[i64]) -> Request {
    generic_request(METHOD_QUEUE_MOVE_TOP, Some(ids))
}

pub fn torrent_queue_move_bottom(ids: &[i64]) -> Request {
    generic_request(METHOD_QUEUE_MOVE_BOTTOM, Some(ids))
}

pub fn torrent_start_now(ids: &[i64]) -> Request {
    generic_request(METHOD_TORRENT_START_NOW, Some(ids))
}

pub fn torrent_set(ids: &[i64]) -> Request {
    generic_request(METHOD_TORRENT_SET, Some(ids))
}

pub fn torrent_remove(ids: &[i64], remove_data: bool) -> Request {
    let mut req = base_request(METHOD_TORRENT_REMOVE);
    let args = args_mut(&mut req);
    args.insert(PARAM_IDS.into(), json!(ids));
    args.insert(PARAM_DELETE_LOCAL_DATA.into(), json!(remove_data));
    request_set_tag(&mut req, TORRENT_GET_TAG_MODE_FULL);
    req
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
    FIELD_ANNOUNCE_URL,
    FIELD_ERROR_STRING,
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
];

/// A torrent-get of all the torrents (TORRENT_GET_TAG_MODE_FULL), the
/// recently active ones (TORRENT_GET_TAG_MODE_UPDATE), or the one with id.
pub fn torrent_get(id: i64) -> Request {
    let mut req = base_request(METHOD_TORRENT_GET);
    let args = args_mut(&mut req);
    if id == TORRENT_GET_TAG_MODE_UPDATE {
        args.insert(PARAM_IDS.into(), json!(FIELD_RECENTLY_ACTIVE));
    } else if id >= 0 {
        args.insert(PARAM_IDS.into(), json!([id]));
    }
    args.insert(PARAM_FIELDS.into(), json!(TORRENT_GET_FIELDS));
    req
}

pub fn torrent_add_url(url: &str, paused: bool) -> Request {
    let mut req = base_request(METHOD_TORRENT_ADD);
    let args = args_mut(&mut req);
    args.insert(PARAM_FILENAME.into(), json!(url));
    args.insert(PARAM_PAUSED.into(), json!(paused));
    request_set_tag(&mut req, TORRENT_GET_TAG_MODE_FULL);
    req
}

/// torrent-add for a file (sent base64 encoded), URL or magnet link. None
/// (with a message) if the file doesn't exist; an error if it can't be read.
pub fn torrent_add_from_file(target: &str, flags: u32) -> Result<Option<Request>, String> {
    let is_uri = crate::util::is_magnet(target) || crate::util::is_url(target);

    if !is_uri && !std::path::Path::new(target).is_file() {
        glib_message(&format!("file \"{target}\" does not exist."));
        return Ok(None);
    }

    let mut req = base_request(METHOD_TORRENT_ADD);
    let args = args_mut(&mut req);

    if is_uri {
        args.insert(PARAM_FILENAME.into(), json!(target));
    } else {
        let encoded = crate::util::base64_encode_file(target)?;
        args.insert(PARAM_METAINFO.into(), json!(encoded));
    }

    args.insert(PARAM_PAUSED.into(), json!(flags & TORRENT_ADD_FLAG_PAUSED != 0));

    if flags & TORRENT_ADD_FLAG_DELETE != 0 {
        let _ = std::fs::remove_file(target);
    }

    Ok(Some(req))
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

pub fn torrent_get_flags(
    t: &Obj,
    rpcv: i64,
    status: i64,
    file_count: i64,
    down_rate: i64,
    up_rate: i64,
) -> u32 {
    let mut flags = 0;

    if file_count > 0 && torrent_get_is_finished(t) {
        flags |= TORRENT_FLAG_COMPLETE;
    } else {
        flags |= TORRENT_FLAG_INCOMPLETE;
    }

    if rpcv >= NEW_STATUS_RPC_VERSION {
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
    } else {
        match status {
            OLD_STATUS_DOWNLOADING => flags |= TORRENT_FLAG_DOWNLOADING,
            OLD_STATUS_PAUSED => flags |= TORRENT_FLAG_PAUSED,
            OLD_STATUS_SEEDING => flags |= TORRENT_FLAG_SEEDING,
            OLD_STATUS_CHECKING => flags |= TORRENT_FLAG_CHECKING,
            OLD_STATUS_WAITING_TO_CHECK => {
                flags |= TORRENT_FLAG_WAITING_CHECK | TORRENT_FLAG_CHECKING
            }
            _ => {}
        }
        if down_rate > 0 || up_rate > 0 {
            flags |= TORRENT_FLAG_ACTIVE;
        }
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

pub fn torrent_get_status_string(rpcv: i64, value: i64, flags: u32) -> String {
    let metadata = flags & TORRENT_FLAG_DOWNLOADING_METADATA != 0;
    if rpcv >= NEW_STATUS_RPC_VERSION {
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
    } else {
        match value {
            OLD_STATUS_DOWNLOADING if metadata => tr("Metadata Downloading"),
            OLD_STATUS_DOWNLOADING => tr("Downloading"),
            OLD_STATUS_PAUSED => tr("Paused"),
            OLD_STATUS_SEEDING => tr("Seeding"),
            OLD_STATUS_CHECKING => tr("Checking"),
            OLD_STATUS_WAITING_TO_CHECK => tr("Waiting To Check"),
            _ => tr("Unknown"),
        }
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

pub fn session_get_cache_size_mb(s: &Obj) -> i64 {
    if s.contains_key(SGET_CACHE_SIZE_MB) { get_i64(s, SGET_CACHE_SIZE_MB) } else { -1 }
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
}

/// Adds a file index to the array args[key], creating it if needed
pub fn add_file_id_to_array(args: &mut Obj, key: &str, index: i64) {
    let entry = args.entry(key.to_string()).or_insert_with(|| json!([]));
    if let Some(a) = entry.as_array_mut() {
        a.push(json!(index));
    }
}
