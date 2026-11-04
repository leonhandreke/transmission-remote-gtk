//! Adding torrents one after the other (upload.c): files, URLs or magnet
//! links, optionally with a destination, priority and the files' wanted
//! states and priorities.

use crate::client::{Client, Response};
use crate::main_window::MainWindow;
use crate::protocol::{self as p, TR_PRI_HIGH, TR_PRI_LOW};
use crate::util;
use serde_json::json;

pub struct Upload {
    /// the filenames (or URLs)
    pub list: Vec<String>,
    pub client: Client,
    /// a parent window to attach any error dialogs to
    pub main_window: MainWindow,
    pub flags: u32,
    pub dir: Option<String>,
    pub priority: i64,
    pub file_priorities: Option<Vec<i64>>,
    pub file_wanted: Option<Vec<bool>>,
    pub extra_args: bool,
    pub progress_index: usize,
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
            file_priorities: None,
            file_wanted: None,
            extra_args: false,
            progress_index: 0,
        }
    }
}

fn next_upload(mut upload: Upload) {
    let Some(target) = upload.list.get(upload.progress_index).cloned() else {
        return;
    };

    let req = match p::torrent_add_from_file(&target, upload.flags) {
        Ok(Some(req)) => req,
        Ok(None) => return,
        Err(e) => {
            util::error_dialog(Some(&upload.main_window), &e);
            return;
        }
    };

    let mut req = req;
    let args = p::args_mut(&mut req);

    if upload.extra_args {
        args.insert(
            p::FIELD_FILE_DOWNLOAD_DIR.into(),
            json!(upload.dir.clone().unwrap_or_default()),
        );
        args.insert(p::FIELD_BANDWIDTH_PRIORITY.into(), json!(upload.priority));
    }

    if let Some(wanted) = &upload.file_wanted {
        for (i, w) in wanted.iter().enumerate() {
            let key = if *w { p::FIELD_FILES_WANTED } else { p::FIELD_FILES_UNWANTED };
            p::add_file_id_to_array(args, key, i as i64);
        }
    }

    if let Some(priorities) = &upload.file_priorities {
        for (i, pr) in priorities.iter().enumerate() {
            let key = match *pr {
                TR_PRI_LOW => p::FIELD_FILES_PRIORITY_LOW,
                TR_PRI_HIGH => p::FIELD_FILES_PRIORITY_HIGH,
                _ => p::FIELD_FILES_PRIORITY_NORMAL,
            };
            p::add_file_id_to_array(args, key, i as i64);
        }
    }

    upload.progress_index += 1;
    let client = upload.client.clone();
    client.dispatch(req, move |response: Response| {
        upload.main_window.on_generic_interactive_action(response);
        next_upload(upload);
    });
}

pub fn do_upload(upload: Upload) {
    next_upload(upload);
}
