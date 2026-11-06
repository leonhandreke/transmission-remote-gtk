//! transmission-remote-gtk - A GTK RPC client to Transmission
//! Copyright (C) 2011-2013  Alan Fitton
//!
//! This program is free software; you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the
//! Free Software Foundation; either version 2 of the License, or (at your
//! option) any later version.
//!
//! This program is distributed in the hope that it will be useful, but
//! WITHOUT ANY WARRANTY; without even the implied warranty of
//! MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General
//! Public License for more details.

mod app;
mod bencode;
mod client;
#[cfg(feature = "debug-snapshot")]
mod debug_snapshot;
mod dialog;
mod dialogs;
mod files;
mod i18n;
mod item;
mod macos;
mod main_window;
mod menus;
mod panels;
mod peers;
mod prefs;
mod protocol;
mod rpc;
mod state_selector;
mod torrent_model;
mod torrent_view;
mod trackers;
mod tree_view;
mod upload;
mod util;

use gtk::prelude::*;
use gtk::{gio, glib};

/// Binds the translations: those of a bundle if we're run from one (also for
/// GTK, which binds its translations to its build-time prefix on
/// initialisation), otherwise those of the prefix.
fn bindtext_wrapper() {
    #[cfg(windows)]
    {
        let dir = std::env::current_exe().ok().and_then(|e| {
            e.parent().and_then(|d| d.parent()).map(|d| d.join("share").join("locale"))
        });
        if let Some(dir) = dir {
            i18n::init(&dir.to_string_lossy());
            return;
        }
    }

    match macos::bundle_locale_dir().filter(|_| cfg!(target_os = "macos")) {
        Some(dir) => {
            let dir = dir.to_string_lossy();
            i18n::init(&dir);
            if gtk::init().is_ok() {
                i18n::bind("gtk40", &dir);
            }
        }
        None => i18n::init(env!("TRG_LOCALEDIR")),
    }
}

fn main() -> glib::ExitCode {
    if cfg!(target_os = "macos") {
        macos::setup_environment();
    }

    // the user's locale, for the translations (GTK also does this when it
    // initialises)
    unsafe {
        libc::setlocale(libc::LC_ALL, c"".as_ptr());
    }

    glib::set_application_name(env!("TRG_PACKAGE_NAME"));
    glib::set_prgname(Some(env!("TRG_APPLICATION_ID")));
    bindtext_wrapper();

    gio::resources_register_include!("trg-ui.gresource").expect("the user interface is built in");
    gio::resources_register_include!("trg-data.gresource").expect("the icons are built in");

    let client = client::Client::new();
    let app = app::App::new(&client);
    app.run()
}
