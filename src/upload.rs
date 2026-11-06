//! Adding torrents (upload.c): files, URLs or magnet links, optionally with
//! a destination, priority, sequential download and the files' wanted
//! states and priorities. All of them go in one batch, then the added
//! torrents are fetched.

use gtk::prelude::*;

use crate::client::{Client, Response};
use crate::i18n::{sprintf, tr};
use crate::main_window::MainWindow;
use crate::protocol::{self as p, TR_PRI_HIGH, TR_PRI_LOW};
use crate::util;

pub struct Upload {
    /// the filenames (or URLs)
    pub list: Vec<String>,
    pub client: Client,
    /// a parent window to attach any error dialogs to
    pub main_window: MainWindow,
    pub flags: u32,
    pub dir: Option<String>,
    pub priority: i64,
    pub sequential_download: Option<bool>,
    pub file_priorities: Option<Vec<i64>>,
    pub file_wanted: Option<Vec<bool>>,
    pub extra_args: bool,
}

impl Upload {
    pub fn new(list: Vec<String>, client: &Client, main_window: &MainWindow, flags: u32) -> Self {
        Self {
            list,
            client: client.clone(),
            main_window: main_window.clone(),
            flags,
            dir: None,
            priority: 0,
            sequential_download: None,
            file_priorities: None,
            file_wanted: None,
            extra_args: false,
        }
    }

    fn request(&self, target: &str) -> Result<Option<p::Request>, String> {
        let Some(mut req) = p::torrent_add_from_file(target, self.flags)? else {
            return Ok(None);
        };

        if self.extra_args {
            req.set(p::FIELD_FILE_DOWNLOAD_DIR, self.dir.clone().unwrap_or_default());
            req.set(p::FIELD_BANDWIDTH_PRIORITY, self.priority);
        }
        if let Some(sequential) = self.sequential_download {
            req.set(p::FIELD_SEQUENTIAL_DOWNLOAD, sequential);
        }

        if let Some(wanted) = &self.file_wanted {
            for (i, w) in wanted.iter().enumerate() {
                let key = if *w { p::FIELD_FILES_WANTED } else { p::FIELD_FILES_UNWANTED };
                p::add_file_id_to_array(&mut req, key, i as i64);
            }
        }

        if let Some(priorities) = &self.file_priorities {
            for (i, pr) in priorities.iter().enumerate() {
                let key = match *pr {
                    TR_PRI_LOW => p::FIELD_FILES_PRIORITY_LOW,
                    TR_PRI_HIGH => p::FIELD_FILES_PRIORITY_HIGH,
                    _ => p::FIELD_FILES_PRIORITY_NORMAL,
                };
                p::add_file_id_to_array(&mut req, key, i as i64);
            }
        }

        Ok(Some(req))
    }
}

pub fn do_upload(upload: Upload) {
    let mut reqs = Vec::new();
    let mut errors = Vec::new();
    for target in &upload.list {
        match upload.request(target) {
            Ok(Some(req)) => reqs.push(req),
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        util::error_dialog(Some(&upload.main_window), &errors.join("\n"));
    }
    dispatch_adds(&upload.main_window, &upload.client, reqs);
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
            win.refresh_torrents(&p::Torrents::Ids(ids));
        }
        if !messages.is_empty() {
            util::error_dialog(Some(&win), &messages.join("\n"));
        }
    });
}
