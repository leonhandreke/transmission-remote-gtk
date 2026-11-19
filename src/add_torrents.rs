//! Adding torrents: files, URLs or magnet links, optionally with a
//! destination, priority, sequential download and the files' wanted states
//! and priorities. All of them go in one batch, then the added torrents are
//! fetched.

use std::path::Path;

use gtk::glib;
use gtk::prelude::*;

use crate::client::{Client, Response};
use crate::i18n::{sprintf, tr};
use crate::main_window::MainWindow;
use crate::prefs::{self, Prefs, Scope};
use crate::protocol::{self as p, TR_PRI_HIGH, TR_PRI_LOW};
use crate::util;

/// How torrents are added
#[derive(Debug, Default, Clone)]
pub struct AddOptions {
    pub paused: bool,
    /// Delete the local .torrent files once they're read
    pub delete_local: bool,
    pub download_dir: Option<String>,
    pub bandwidth_priority: Option<i64>,
    pub sequential_download: Option<bool>,
    /// Whether each file of the torrent is wanted
    pub files_wanted: Option<Vec<bool>>,
    /// The priority of each file of the torrent
    pub file_priorities: Option<Vec<i64>>,
}

impl AddOptions {
    /// The options of the preferences, for adding without the add dialog
    pub fn from_prefs(prefs: &Prefs) -> Self {
        Self {
            paused: prefs.get_bool(prefs::KEY_START_PAUSED, Scope::Global),
            delete_local: prefs.get_bool(prefs::KEY_DELETE_LOCAL_TORRENT, Scope::Global),
            ..Default::default()
        }
    }

    /// The request adding a file, URL or magnet link. None (with a message)
    /// if the file doesn't exist; an error if it can't be read.
    fn request(&self, target: &str) -> Result<Option<p::Request>, String> {
        let mut req = if util::is_magnet(target) || util::is_url(target) {
            p::torrent_add_url(target, self.paused)
        } else if Path::new(target).is_file() {
            let metainfo = std::fs::read(target).map_err(|e| e.to_string())?;
            if self.delete_local {
                let _ = std::fs::remove_file(target);
            }
            p::torrent_add_metainfo(&metainfo, self.paused)
        } else {
            glib::g_message!("trg", "file \"{target}\" does not exist.");
            return Ok(None);
        };

        if let Some(dir) = &self.download_dir {
            req.set(p::FIELD_FILE_DOWNLOAD_DIR, dir);
        }
        if let Some(priority) = self.bandwidth_priority {
            req.set(p::FIELD_BANDWIDTH_PRIORITY, priority);
        }
        if let Some(sequential) = self.sequential_download {
            req.set(p::FIELD_SEQUENTIAL_DOWNLOAD, sequential);
        }

        for (i, wanted) in self.files_wanted.iter().flatten().enumerate() {
            let key = if *wanted { p::FIELD_FILES_WANTED } else { p::FIELD_FILES_UNWANTED };
            p::add_file_id_to_array(&mut req, key, i as i64);
        }

        for (i, priority) in self.file_priorities.iter().flatten().enumerate() {
            let key = match *priority {
                TR_PRI_LOW => p::FIELD_FILES_PRIORITY_LOW,
                TR_PRI_HIGH => p::FIELD_FILES_PRIORITY_HIGH,
                _ => p::FIELD_FILES_PRIORITY_NORMAL,
            };
            p::add_file_id_to_array(&mut req, key, i as i64);
        }

        Ok(Some(req))
    }
}

/// Adds torrents (files, URLs or magnet links). Errors are shown on win.
pub fn add_torrents(win: &MainWindow, client: &Client, targets: &[String], options: &AddOptions) {
    let mut reqs = Vec::new();
    let mut errors = Vec::new();
    for target in targets {
        match options.request(target) {
            Ok(Some(req)) => reqs.push(req),
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        util::error_dialog(Some(win), &errors.join("\n"));
    }
    dispatch_adds(win, client, reqs);
}

/// Sends torrent_add requests in a batch, then gets the added torrents.
/// Shows the errors, and the torrents that were there already.
pub fn dispatch_adds(win: &MainWindow, client: &Client, reqs: Vec<p::Request>) {
    if reqs.is_empty() {
        return;
    }
    let weak = win.downgrade();
    client.dispatch_batch(reqs, move |results: Vec<Response>| {
        let Some(win) = weak.upgrade() else { return };
        let mut ids = Vec::new();
        let mut messages = Vec::new();
        for result in &results {
            match result {
                Ok(r) => match p::torrent_added(r) {
                    Some((t, false)) => ids.push(p::torrent_get_id(t)),
                    Some((t, true)) => messages.push(sprintf(
                        &tr("\"%s\" has already been added."),
                        &[&p::torrent_get_name(t)],
                    )),
                    None => {}
                },
                Err(e) => messages.push(e.to_string()),
            }
        }
        if !ids.is_empty() {
            win.client().refresh_torrents(&p::Torrents::Ids(ids));
        }
        if !messages.is_empty() {
            util::error_dialog(Some(&win), &messages.join("\n"));
        }
    });
}
