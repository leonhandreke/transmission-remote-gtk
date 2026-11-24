//! A torrent as torrent_get returns it, with the fields the application
//! asks for, and what's derived from them: its flags, status and icon.
//!
//! The fields are the keys of the response, and those asked for (see
//! torrent_fields()). Missing fields have their default values.

use std::sync::OnceLock;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::i18n::tr;
use crate::json::{Obj, value_as_bool};

pub const TR_STATUS_STOPPED: i64 = 0;
pub const TR_STATUS_CHECK_WAIT: i64 = 1;
pub const TR_STATUS_CHECK: i64 = 2;
pub const TR_STATUS_DOWNLOAD_WAIT: i64 = 3;
pub const TR_STATUS_DOWNLOAD: i64 = 4;
pub const TR_STATUS_SEED_WAIT: i64 = 5;
pub const TR_STATUS_SEED: i64 = 6;

/* What a torrent is doing, for the views and the filters of the sidebar */
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Torrent {
    pub id: i64,
    pub name: String,
    /// One of TR_STATUS_*
    pub status: i64,
    /// 0 for none, 1 a tracker warning, 2 a tracker error, 3 a local error
    pub error: i64,
    pub error_string: String,
    /// Seconds, or -1 if unknown, -2 if it never ends
    pub eta: i64,
    /// 0 to 1
    pub percent_done: f64,
    /// 0 to 1, missing once the metadata is complete
    pub metadata_percent_complete: Option<f64>,
    /// 0 to 1, while it's being checked
    pub recheck_progress: f64,
    /// Bytes per second
    pub rate_download: i64,
    pub rate_upload: i64,
    pub size_when_done: i64,
    pub total_size: i64,
    pub left_until_done: i64,
    pub have_valid: i64,
    pub have_unchecked: i64,
    pub downloaded_ever: i64,
    pub uploaded_ever: i64,
    pub corrupt_ever: i64,
    /* dates, in seconds since the epoch */
    pub added_date: i64,
    pub done_date: i64,
    pub activity_date: i64,
    pub date_created: i64,
    pub download_dir: String,
    pub hash_string: String,
    pub comment: String,
    pub creator: String,
    pub magnet_link: String,
    pub is_finished: bool,
    pub is_private: bool,
    pub queue_position: Option<i64>,
    /// One of TR_PRI_LOW, TR_PRI_NORMAL and TR_PRI_HIGH
    pub bandwidth_priority: i64,
    pub honors_session_limits: bool,
    pub download_limited: bool,
    /// KiB/s
    pub download_limit: i64,
    pub upload_limited: bool,
    /// KiB/s
    pub upload_limit: i64,
    /// 0: the session's limit, 1: seed_ratio_limit, 2: unlimited
    pub seed_ratio_mode: i64,
    pub seed_ratio_limit: f64,
    pub peer_limit: i64,
    pub peers_connected: i64,
    pub peers_sending_to_us: i64,
    pub peers_getting_from_us: i64,
    pub webseeds_sending_to_us: i64,
    pub peers_from: PeersFrom,
    pub peers: Vec<Peer>,
    pub files: Vec<File>,
    /// Whether each file is downloaded
    #[serde(deserialize_with = "lenient_bools")]
    pub wanted: Vec<bool>,
    /// The priority of each file
    pub priorities: Vec<i64>,
    pub tracker_stats: Vec<TrackerStat>,
    /// Missing for servers without sequential downloading
    pub sequential_download: Option<bool>,
    pub sequential_download_from_piece: i64,
    pub piece_count: i64,
}

/// How many of the connected peers came from where
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct PeersFrom {
    pub from_cache: i64,
    pub from_dht: i64,
    pub from_incoming: i64,
    pub from_lpd: Option<i64>,
    pub from_ltep: i64,
    pub from_pex: i64,
    pub from_tracker: i64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Peer {
    pub address: String,
    pub client_name: String,
    pub flag_str: String,
    /// 0 to 1
    pub progress: f64,
    /// Bytes per second
    pub rate_to_client: i64,
    pub rate_to_peer: i64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct File {
    /// The path in the torrent, "dir/sub/file"
    pub name: String,
    pub length: i64,
    pub bytes_completed: i64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct TrackerStat {
    pub id: i64,
    pub tier: i64,
    pub announce: String,
    pub scrape: String,
    /// host:port
    pub host: String,
    pub last_announce_peer_count: i64,
    pub last_announce_result: String,
    pub last_announce_time: i64,
    pub last_scrape_time: i64,
    pub seeder_count: i64,
    pub leecher_count: i64,
    pub download_count: i64,
    pub downloader_count: i64,
}

/// Booleans, also as numbers (0 or 1), as older servers sent them
fn lenient_bools<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<bool>, D::Error> {
    Ok(Vec::<Value>::deserialize(d)?.iter().map(value_as_bool).collect())
}

/// The fields of torrent_get, the keys of Torrent
pub fn torrent_fields() -> &'static [String] {
    static FIELDS: OnceLock<Vec<String>> = OnceLock::new();
    FIELDS.get_or_init(|| match serde_json::to_value(Torrent::default()) {
        Ok(Value::Object(o)) => o.into_iter().map(|(k, _)| k).collect(),
        _ => Vec::new(),
    })
}

/// The torrents of a torrent_get result, and the IDs of the removed ones
/// (for the recently active ones)
pub fn torrents_of(result: Obj) -> (Vec<Torrent>, Vec<i64>) {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct TorrentGet {
        torrents: Vec<Value>,
        removed: Vec<i64>,
    }
    let get: TorrentGet = serde_json::from_value(Value::Object(result)).unwrap_or_default();
    let torrents = get
        .torrents
        .into_iter()
        .filter_map(|t| {
            serde_json::from_value(t)
                .map_err(|e| gtk::glib::g_warning!("trg", "Ignoring a torrent: {e}"))
                .ok()
        })
        .collect();
    (torrents, get.removed)
}

/// The progress in percent of length bytes of which completed are done
pub fn file_progress(length: i64, completed: i64) -> f64 {
    if length > 0 { ((completed as f64 / length as f64) * 100.0).min(100.0) } else { 0.0 }
}

impl Torrent {
    /// Its fields as a JSON object, for widgets that load them by their keys
    pub fn to_obj(&self) -> Obj {
        match serde_json::to_value(self) {
            Ok(Value::Object(o)) => o,
            _ => Obj::new(),
        }
    }

    /// Its progress in percent
    pub fn done_percent(&self) -> f64 {
        self.percent_done * 100.0
    }

    /// The progress of its metadata in percent
    pub fn metadata_percent(&self) -> f64 {
        self.metadata_percent_complete.map_or(100.0, |p| p * 100.0)
    }

    /// The progress of checking it in percent
    pub fn recheck_percent(&self) -> f64 {
        self.recheck_progress * 100.0
    }

    /// Whether all of its wanted files are there
    pub fn is_complete(&self) -> bool {
        self.left_until_done <= 0
    }

    /// TORRENT_FLAG_*
    pub fn flags(&self) -> u32 {
        let has_metadata = !self.files.is_empty();
        let mut flags = if has_metadata && self.is_complete() {
            TORRENT_FLAG_COMPLETE
        } else {
            TORRENT_FLAG_INCOMPLETE
        };

        match self.status {
            TR_STATUS_STOPPED => flags |= TORRENT_FLAG_PAUSED,
            TR_STATUS_CHECK_WAIT => flags |= TORRENT_FLAG_WAITING_CHECK | TORRENT_FLAG_CHECKING_ANY,
            TR_STATUS_CHECK => flags |= TORRENT_FLAG_CHECKING | TORRENT_FLAG_CHECKING_ANY,
            TR_STATUS_DOWNLOAD_WAIT => flags |= TORRENT_FLAG_DOWNLOADING_WAIT | TORRENT_FLAG_QUEUED,
            TR_STATUS_DOWNLOAD => {
                if flags & TORRENT_FLAG_COMPLETE == 0 {
                    flags |= TORRENT_FLAG_DOWNLOADING;
                }
                if !has_metadata {
                    flags |= TORRENT_FLAG_DOWNLOADING_METADATA;
                }
                flags |= TORRENT_FLAG_ACTIVE;
            }
            TR_STATUS_SEED_WAIT => flags |= TORRENT_FLAG_SEEDING_WAIT,
            TR_STATUS_SEED => {
                flags |= TORRENT_FLAG_SEEDING;
                if self.peers_getting_from_us != 0 {
                    flags |= TORRENT_FLAG_ACTIVE;
                }
            }
            _ => {}
        }

        if self.error > 0 {
            flags |= TORRENT_FLAG_ERROR;
        }
        flags
    }

    /// Its status, with its flags
    pub fn status_text(&self, flags: u32) -> String {
        let metadata = flags & TORRENT_FLAG_DOWNLOADING_METADATA != 0;
        match self.status {
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

    /// Whether one of its trackers has the host (see util::tracker_host())
    pub fn has_tracker(&self, host: &str) -> bool {
        self.tracker_stats.iter().any(|t| crate::util::tracker_host(&t.announce) == Some(host))
    }
}

/// The icon of a torrent with flags
pub fn status_icon(flags: u32) -> &'static str {
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
