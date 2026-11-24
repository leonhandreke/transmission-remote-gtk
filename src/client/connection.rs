//! Connecting to the server, and keeping the session and the torrents up to
//! date: they're fetched periodically, and along with the requests that
//! change them. The window follows with the client's signals:
//!
//! - "connection-changed" when it connects, disconnects, or starts
//!   connecting (see connection_state()),
//! - "torrents-updated" with the UpdateMode, once the torrents are updated,
//! - "request-failed" with the message of a failed request, to show,
//! - "status-message" with a message for the status bar.

use gtk::glib;
use gtk::glib::prelude::*;
use gtk::glib::subclass::prelude::*;

use super::{Client, ConnectionState, Error, Response};
use crate::i18n::{sprintf, tr};
use crate::json::get_str;
use crate::prefs::{self, Scope};
use crate::protocol as p;
use crate::torrent_model::UpdateMode;

impl Client {
    pub fn connection_state(&self) -> ConnectionState {
        if self.is_connected() {
            ConnectionState::Connected
        } else if self.imp().connecting.get() {
            ConnectionState::Connecting
        } else {
            ConnectionState::Disconnected
        }
    }

    /// Connects to the profile with index, or the current one for -1. An
    /// error if its settings are bad.
    pub fn connect_to_profile(&self, index: i32) -> Result<(), String> {
        let prefs = self.prefs().clone();
        let current = prefs.profile();
        let profile =
            (index >= 0 && (index as usize) < prefs.n_profiles()).then_some(index as usize);

        if self.is_connected() {
            self.disconnect();
        }

        match profile {
            Some(p) if Some(p) != current => prefs.set_profile(Some(p)),
            _ => prefs.emit_profile_changed(),
        }

        // a new connid drops the responses of an attempt in progress
        self.inc_connid();
        if let Err(msg) = self.parse_settings() {
            self.set_connecting(false);
            return Err(msg);
        }
        self.set_connecting(true);

        // the session and the torrents in one go
        let client = self.clone();
        self.dispatch_batch(
            vec![p::session_get(), p::torrent_get(&p::Torrents::All)],
            move |mut results| {
                let (Some(torrents), Some(session)) = (results.pop(), results.pop()) else {
                    return;
                };
                let was_connected = client.is_connected();
                client.on_session_get(session);
                if !was_connected && client.is_connected() {
                    client.on_torrent_get(torrents, UpdateMode::First);
                }
            },
        );
        Ok(())
    }

    /// Disconnects, or stops connecting
    pub fn disconnect(&self) {
        self.inc_connid();
        self.connection_changed(false);
    }

    /// Sends a request about torrents, and in the same batch, which the
    /// server executes in order, gets them again (all of them if it's about
    /// all of them, the recently active ones and removed IDs for a
    /// removal).
    pub fn dispatch_interactive(&self, req: p::Request) {
        self.dispatch_interactive_with(req, |_| {});
    }

    /// dispatch_interactive(), after() gets the result of the request first
    pub fn dispatch_interactive_with(
        &self,
        req: p::Request,
        after: impl FnOnce(&Response) + 'static,
    ) {
        let refresh = if req.method == p::METHOD_TORRENT_REMOVE {
            p::Torrents::RecentlyActive
        } else {
            p::Torrents::of(&req)
        };
        let client = self.clone();
        self.dispatch_batch(vec![req, p::torrent_get(&refresh)], move |mut results| {
            let (Some(torrents), Some(result)) = (results.pop(), results.pop()) else { return };
            after(&result);
            if !client.is_connected() {
                return;
            }
            if let Err(e) = &result {
                client.request_failed(e);
            }
            client.on_torrent_get(torrents, UpdateMode::Interaction);
        });
    }

    /// Sends a session_set, and in the same batch gets the session again
    pub fn dispatch_session_set(&self, req: p::Request) {
        let client = self.clone();
        self.update_session_after(vec![req], move |results, session| {
            if let Some(Err(e)) = results.first() {
                client.request_failed(e);
            }
            client.on_session_get(session);
        });
    }

    /// Gets torrents (after they changed outside of a batch, such as adding
    /// them)
    pub fn refresh_torrents(&self, torrents: &p::Torrents) {
        let client = self.clone();
        self.dispatch(p::torrent_get(torrents), move |r| {
            client.on_torrent_get(r, UpdateMode::Interaction);
        });
    }

    /// Gets the session (after something changed it, such as deleting data)
    pub fn refresh_session(&self) {
        let client = self.clone();
        self.update_session(move |r| client.on_session_get(r));
    }

    /// Shortens the torrents' download directories again (the destinations
    /// or the session's download directory changed)
    pub fn reload_dir_aliases(&self) {
        self.torrent_model().reload_dir_aliases(self);
    }

    pub fn connect_connection_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_local("connection-changed", false, move |args| {
            f(&args[0].get::<Self>().unwrap());
            None
        });
    }

    pub fn connect_torrents_updated<F: Fn(&Self, UpdateMode) + 'static>(&self, f: F) {
        self.connect_local("torrents-updated", false, move |args| {
            f(&args[0].get::<Self>().unwrap(), args[1].get::<UpdateMode>().unwrap());
            None
        });
    }

    pub fn connect_request_failed<F: Fn(&Self, &str) + 'static>(&self, f: F) {
        self.connect_local("request-failed", false, move |args| {
            f(&args[0].get::<Self>().unwrap(), &args[1].get::<String>().unwrap());
            None
        });
    }

    pub fn connect_status_message<F: Fn(&Self, &str) + 'static>(&self, f: F) {
        self.connect_local("status-message", false, move |args| {
            f(&args[0].get::<Self>().unwrap(), &args[1].get::<String>().unwrap());
            None
        });
    }

    fn request_failed(&self, error: &Error) {
        self.emit_by_name::<()>("request-failed", &[&error.to_string()]);
    }

    fn set_connecting(&self, connecting: bool) {
        self.imp().connecting.set(connecting);
        self.emit_by_name::<()>("connection-changed", &[]);
    }

    /// Connecting succeeded, or the connection ended (or connecting stopped)
    fn connection_changed(&self, connected: bool) {
        let imp = self.imp();
        imp.connecting.set(false);
        if connected {
            self.start_session_timer();
        } else {
            self.torrent_model().remove_all();
            for timer in [&imp.torrents_timer, &imp.session_timer] {
                if let Some(id) = timer.take() {
                    id.remove();
                }
            }
            *imp.session.borrow_mut() = None;
            self.prefs().set_connection(None);
        }
        self.emit_by_name::<()>("connection-changed", &[]);
    }

    fn start_session_timer(&self) {
        let interval =
            self.prefs().get_int(prefs::KEY_SESSION_UPDATE_INTERVAL, Scope::Connection).max(1);
        let weak = self.downgrade();
        let id = glib::timeout_add_seconds_local_once(interval as u32, move || {
            let Some(client) = weak.upgrade() else { return };
            // a one-shot timer, so the id is no longer valid
            client.imp().session_timer.take();
            let c = client.clone();
            client.update_session(move |r| {
                c.on_session_get(r);
                c.start_session_timer();
            });
        });
        if let Some(old) = self.imp().session_timer.replace(Some(id)) {
            old.remove();
        }
    }

    fn start_torrents_timer(&self, interval: u32) {
        let weak = self.downgrade();
        let id = glib::timeout_add_seconds_local_once(interval, move || {
            if let Some(client) = weak.upgrade() {
                client.imp().torrents_timer.take();
                client.update_torrents();
            }
        });
        if let Some(old) = self.imp().torrents_timer.replace(Some(id)) {
            old.remove();
        }
    }

    /// The periodic update: the recently active torrents, or all of them
    fn update_torrents(&self) {
        if !self.is_connected() {
            return;
        }
        let prefs = self.prefs();
        let every = prefs.get_int(prefs::ACTIVEONLY_FULLSYNC_EVERY, Scope::Connection);
        let active_only = prefs.get_bool(prefs::KEY_UPDATE_ACTIVE_ONLY, Scope::Connection)
            && (!prefs.get_bool(prefs::ACTIVEONLY_FULLSYNC_ENABLED, Scope::Connection)
                || (every != 0 && self.serial() % every != 0));
        let (torrents, mode) = if active_only {
            (p::Torrents::RecentlyActive, UpdateMode::Active)
        } else {
            (p::Torrents::All, UpdateMode::Update)
        };
        let client = self.clone();
        self.dispatch(p::torrent_get(&torrents), move |r| client.on_torrent_get(r, mode));
    }

    fn on_session_get(&self, response: Response) {
        let was_connected = self.is_connected();
        match response {
            // servers older than 4.1 fail with Error::Unsupported
            Err(e) if !was_connected => {
                self.set_connecting(false);
                self.request_failed(&e);
            }
            Err(_) => {}
            Ok(session) => {
                let reload_aliases = self.session().is_some_and(|last| {
                    get_str(&last, p::SGET_DOWNLOAD_DIR) != get_str(&session, p::SGET_DOWNLOAD_DIR)
                });
                self.set_session(session);
                if reload_aliases {
                    self.reload_dir_aliases();
                }
                if !was_connected {
                    self.connection_changed(true);
                }
            }
        }
    }

    fn on_torrent_get(&self, response: Response, mode: UpdateMode) {
        // disconnected between the request and the response
        if !self.is_connected() {
            return;
        }

        let mut interval = self.prefs().get_int(prefs::KEY_UPDATE_INTERVAL, Scope::Connection);
        if interval < 1 {
            interval = prefs::INTERVAL_DEFAULT;
        }
        let interval = interval as u32;

        let torrents = match response {
            Ok(torrents) => torrents,
            Err(error) => {
                self.on_torrent_get_failed(&error, interval);
                return;
            }
        };

        self.reset_fail_count();
        self.inc_serial();
        self.torrent_model().update(self, torrents, mode);
        self.emit_by_name::<()>("torrents-updated", &[&mode]);

        if mode != UpdateMode::Interaction {
            self.start_torrents_timer(interval);
        }
    }

    /// A failed torrent_get: retried with the next update, until there were
    /// too many failures, which disconnects
    fn on_torrent_get_failed(&self, error: &Error, interval: u32) {
        let max_retries = self.prefs().get_int(prefs::KEY_RETRIES, Scope::Connection);
        if self.inc_fail_count() as i64 >= max_retries {
            self.connection_changed(false);
            self.request_failed(error);
        } else {
            let msg = sprintf(
                &tr("Request %d/%d failed: %s"),
                &[&self.fail_count(), &max_retries, &error.to_string()],
            );
            self.emit_by_name::<()>("status-message", &[&msg]);
            self.start_torrents_timer(interval);
        }
    }
}
