//! The client of a Transmission server, passed around a lot, as in the C
//! version. It:
//!
//! 1. holds the preferences,
//! 2. holds the RPC transport (rpc::Transport), for connection reuse,
//! 3. has the torrents (the torrent model), and finds them by their ID,
//! 4. dispatches asynchronous requests (and batches of them), unless the
//!    connection changed in the meantime,
//! 5. connects, and keeps the session and the torrents up to date (see
//!    connection.rs), with the connection state, an update serial and the
//!    failures,
//! 6. holds the latest session object of a session_get response, with the
//!    free space of the download directory (from free_space), and emits
//!    "session-updated" with it.

use std::cell::{Cell, RefCell};

use gtk::glib;
use gtk::glib::prelude::*;
use gtk::glib::subclass::prelude::*;

use crate::i18n::tr;
use crate::json::{get_str, objects};
use crate::prefs::{self, Prefs, Scope};
use crate::protocol::{self, Request, Session};
use crate::rpc::{self, Config, Transport};
use crate::torrent_model::{TorrentModel, UpdateMode};
use crate::torrent_object::TorrentObject;

mod connection;

pub use crate::rpc::Error;

/// The result of a request
pub type Response = rpc::Result;

/// The state of the connection to the server, as the window shows it
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

/// Why the connection settings of a profile can't be used
#[derive(Debug)]
pub enum SettingsError {
    NoHostname,
    /// The URL made of the host, port and path isn't valid (GLib's message)
    BadUrl(String),
}

// Not with thiserror: xgettext doesn't find the strings in attributes
impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SettingsError::NoHostname => f.write_str(&tr("No hostname set")),
            SettingsError::BadUrl(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for SettingsError {}

mod imp {
    use super::*;
    use std::sync::OnceLock;

    pub struct Client {
        pub prefs: Prefs,
        pub transport: Transport,
        pub connid: Cell<i64>,
        pub fail_count: Cell<u32>,
        pub update_serial: Cell<i64>,
        pub session: RefCell<Option<Session>>,
        pub torrent_model: TorrentModel,
        /// From connect_to_profile() until the connection succeeds or fails
        pub connecting: Cell<bool>,
        pub torrents_timer: RefCell<Option<glib::SourceId>>,
        pub session_timer: RefCell<Option<glib::SourceId>>,
    }

    impl Default for Client {
        fn default() -> Self {
            Self {
                prefs: Prefs::new(),
                transport: Transport::new(env!("TRG_PACKAGE_NAME")),
                connid: Default::default(),
                fail_count: Default::default(),
                update_serial: Default::default(),
                session: Default::default(),
                torrent_model: TorrentModel::new(),
                connecting: Default::default(),
                torrents_timer: Default::default(),
                session_timer: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Client {
        const NAME: &'static str = "TrgClient";
        type Type = super::Client;
    }

    impl ObjectImpl for Client {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("session-updated").build(),
                    // see connection.rs for these
                    glib::subclass::Signal::builder("connection-changed").build(),
                    glib::subclass::Signal::builder("torrents-updated")
                        .param_types([UpdateMode::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("request-failed")
                        .param_types([String::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("status-message")
                        .param_types([String::static_type()])
                        .build(),
                ]
            })
        }

        fn dispose(&self) {
            self.transport.abort();
        }
    }
}

glib::wrapper! {
    pub struct Client(ObjectSubclass<imp::Client>);
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        let client: Self = glib::Object::new();
        client.prefs().load();
        client
    }

    pub fn prefs(&self) -> &Prefs {
        &self.imp().prefs
    }

    pub fn connect_session_updated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("session-updated", false, move |args| {
            f(&args[0].get::<Self>().unwrap());
            None
        })
    }

    pub fn session(&self) -> Option<std::cell::Ref<'_, Session>> {
        std::cell::Ref::filter_map(self.imp().session.borrow(), Option::as_ref).ok()
    }

    pub fn session_clone(&self) -> Option<Session> {
        self.imp().session.borrow().clone()
    }

    /// The download directory of the session, if there is one
    fn download_dir(&self) -> Option<String> {
        self.session().map(|s| s.download_dir.clone()).filter(|d| !d.is_empty())
    }

    pub fn set_session(&self, mut session: Session) {
        // the free space of the last update, if it isn't in this one
        if session.download_dir_free_space.is_none()
            && let Some(last) = self.session()
            && last.download_dir == session.download_dir
        {
            session.download_dir_free_space = last.download_dir_free_space;
        }
        let needs_free_space = session.download_dir_free_space.is_none();
        let dir = session.download_dir.clone();

        *self.imp().session.borrow_mut() = Some(session);
        self.emit_by_name::<()>("session-updated", &[]);

        // a new download directory (or the first session): its free space
        // now rather than with the next session update
        if needs_free_space && !dir.is_empty() {
            let client = self.clone();
            self.dispatch(protocol::free_space(&dir), move |r| {
                let Ok(r) = r else { return };
                {
                    let mut session = client.imp().session.borrow_mut();
                    let Some(s) = session.as_mut().filter(|s| s.download_dir == dir) else {
                        return;
                    };
                    s.download_dir_free_space = Some(protocol::free_space_bytes(&r));
                }
                client.emit_by_name::<()>("session-updated", &[]);
            });
        }
    }

    pub fn version_string(&self) -> Option<String> {
        self.session().map(|s| s.version.clone())
    }

    pub fn is_connected(&self) -> bool {
        self.imp().session.borrow().is_some()
    }

    fn inc_connid(&self) {
        let c = &self.imp().connid;
        c.set(c.get() + 1);
    }

    fn connid(&self) -> i64 {
        self.imp().connid.get()
    }

    pub fn serial(&self) -> i64 {
        self.imp().update_serial.get()
    }

    pub fn inc_serial(&self) {
        let s = &self.imp().update_serial;
        s.set(s.get() + 1);
    }

    fn fail_count(&self) -> u32 {
        self.imp().fail_count.get()
    }

    fn inc_fail_count(&self) -> u32 {
        let c = &self.imp().fail_count;
        c.set(c.get() + 1);
        c.get()
    }

    fn reset_fail_count(&self) {
        self.imp().fail_count.set(0);
    }

    /// The session's seed ratio limit, if it's on
    pub fn seed_ratio_limit(&self) -> Option<f64> {
        self.session().filter(|s| s.seed_ratio_limited).map(|s| s.seed_ratio_limit)
    }

    /// The torrents
    pub fn torrent_model(&self) -> TorrentModel {
        self.imp().torrent_model.clone()
    }

    /// Looks up a torrent by its ID
    pub fn find_torrent(&self, id: i64) -> Option<TorrentObject> {
        self.torrent_model().find(id)
    }

    /// Reads the connection settings of the current profile, which becomes
    /// the connection.
    pub fn parse_settings(&self) -> Result<(), SettingsError> {
        let prefs = self.prefs();
        prefs.set_connection(prefs.profile());
        let transport = &self.imp().transport;
        transport.configure(None);

        let port = prefs.get_int(prefs::KEY_PORT, Scope::Connection);
        let host = prefs.get_string(prefs::KEY_HOSTNAME, Scope::Connection).unwrap_or_default();
        let path = prefs.get_string(prefs::KEY_RPC_URL_PATH, Scope::Connection).unwrap_or_default();

        if host.is_empty() {
            return Err(SettingsError::NoHostname);
        }

        let ssl = prefs.get_bool(prefs::KEY_SSL, Scope::Connection);

        let url = format!("{}://{host}:{port}{path}", if ssl { "https" } else { "http" });
        if let Err(e) = glib::Uri::parse(&url, glib::UriFlags::NONE) {
            return Err(SettingsError::BadUrl(e.message().to_string()));
        }

        let mut headers = Vec::new();
        if let Some(custom) = prefs.get_array(prefs::KEY_CUSTOM_HEADERS, Scope::Connection) {
            for h in objects(&custom) {
                if let (Some(k), Some(v)) = (
                    get_str(h, prefs::KEY_CUSTOM_HEADER_NAME),
                    get_str(h, prefs::KEY_CUSTOM_HEADER_VALUE),
                ) {
                    headers.push((k.to_string(), v.to_string()));
                }
            }
        }

        transport.configure(Some(Config {
            url,
            username: prefs.get_string(prefs::KEY_USERNAME, Scope::Connection),
            password: prefs.get_string(prefs::KEY_PASSWORD, Scope::Connection),
            headers,
            ssl_validate: prefs.get_bool(prefs::KEY_SSL_VALIDATE, Scope::Connection),
        }));
        Ok(())
    }

    /// Sends requests in a batch, calls callback with their results (in the
    /// same order), unless the client connected or disconnected in the
    /// meantime.
    pub fn dispatch_batch<F: FnOnce(Vec<Response>) + 'static>(
        &self,
        reqs: Vec<Request>,
        callback: F,
    ) {
        let connid = self.connid();
        let client = self.clone();
        glib::spawn_future_local(async move {
            let results = client.imp().transport.batch(reqs).await;
            if client.connid() == connid {
                callback(results);
            }
        });
    }

    /// Sends a request, calls callback with its result, unless the client
    /// connected or disconnected in the meantime.
    pub fn dispatch<F: FnOnce(Response) + 'static>(&self, req: Request, callback: F) {
        self.dispatch_batch(vec![req], move |mut r| {
            callback(r.pop().unwrap_or_else(|| Err(Error::Decode(String::new()))))
        });
    }

    /// Sends requests, then gets the session (and the free space of its
    /// download directory) in the same batch. callback gets the results of
    /// the requests and the session.
    pub fn update_session_after<F>(&self, mut reqs: Vec<Request>, callback: F)
    where
        F: FnOnce(Vec<Response>, rpc::Result<Session>) + 'static,
    {
        let n = reqs.len();
        reqs.push(protocol::session_get());
        let dir = self.download_dir();
        if let Some(dir) = &dir {
            reqs.push(protocol::free_space(dir));
        }
        self.dispatch_batch(reqs, move |mut results| {
            let free_space = if dir.is_some() { results.pop() } else { None };
            let session = results.pop().unwrap_or_else(|| Err(Error::Decode(String::new())));
            let mut session = session.and_then(Session::of);
            if let (Ok(s), Some(Ok(free))) = (&mut session, free_space) {
                // only if the directory didn't change in the meantime
                if Some(&s.download_dir) == dir.as_ref() {
                    s.download_dir_free_space = Some(protocol::free_space_bytes(&free));
                }
            }
            results.truncate(n);
            callback(results, session);
        });
    }

    pub fn update_session<F: FnOnce(rpc::Result<Session>) + 'static>(&self, callback: F) {
        self.update_session_after(Vec::new(), |_, session| callback(session));
    }
}
