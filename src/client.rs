//! The client of a Transmission server, passed around a lot, as in the C
//! version. It:
//!
//! 1. holds the preferences,
//! 2. holds the RPC transport (rpc::Transport), for connection reuse,
//! 3. finds torrents by their ID (in the torrent model),
//! 4. dispatches asynchronous requests (and batches of them), unless the
//!    connection changed in the meantime, and tracks failures,
//! 5. holds the connection state and an update serial,
//! 6. holds the latest session object of a session_get response, with the
//!    free space of the download directory (from free_space), and emits
//!    "session-updated" with it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::glib;
use gtk::glib::prelude::*;
use gtk::glib::subclass::prelude::*;
use serde_json::json;

use crate::prefs::{self, Prefs};
use crate::protocol::{self, Obj, Request};
use crate::rpc::{self, Config, Transport};
use crate::torrent_model::{TorrentItem, TorrentItemExt, TorrentModel};

pub use crate::rpc::Error;

/// The result of a request
pub type Response = rpc::Result;

mod imp {
    use super::*;
    use std::sync::OnceLock;

    pub struct Client {
        pub prefs: Prefs,
        pub transport: Transport,
        pub connid: Cell<i64>,
        pub fail_count: Cell<u32>,
        pub update_serial: Cell<i64>,
        pub session: RefCell<Option<Obj>>,
        pub version: Cell<f64>,
        pub ssl: Cell<bool>,
        pub seed_ratio_limited: Cell<bool>,
        pub seed_ratio_limit: Cell<f64>,
        pub torrent_model: RefCell<Option<TorrentModel>>,
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
                version: Default::default(),
                ssl: Default::default(),
                seed_ratio_limited: Default::default(),
                seed_ratio_limit: Default::default(),
                torrent_model: Default::default(),
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
            SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("session-updated").build()])
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

    pub fn session(&self) -> Option<std::cell::Ref<'_, Obj>> {
        std::cell::Ref::filter_map(self.imp().session.borrow(), Option::as_ref).ok()
    }

    pub fn session_clone(&self) -> Option<Obj> {
        self.imp().session.borrow().clone()
    }

    fn download_dir(&self) -> Option<String> {
        self.session()
            .and_then(|s| protocol::get_str(&s, protocol::SGET_DOWNLOAD_DIR).map(str::to_string))
    }

    pub fn set_session(&self, mut session: Obj) {
        let imp = self.imp();
        if imp.session.borrow().is_none() {
            imp.version.set(protocol::session_get_version(Some(&session)));
        }

        // the free space of the last update, if it isn't in this one
        let dir = protocol::get_str(&session, protocol::SGET_DOWNLOAD_DIR).map(str::to_string);
        if !session.contains_key(protocol::SGET_DOWNLOAD_DIR_FREE_SPACE)
            && dir.is_some()
            && self.download_dir() == dir
        {
            let last =
                self.session().and_then(|s| s.get(protocol::SGET_DOWNLOAD_DIR_FREE_SPACE).cloned());
            if let Some(free) = last {
                session.insert(protocol::SGET_DOWNLOAD_DIR_FREE_SPACE.into(), free);
            }
        }
        let needs_free_space = !session.contains_key(protocol::SGET_DOWNLOAD_DIR_FREE_SPACE);

        imp.seed_ratio_limit.set(protocol::get_f64(&session, protocol::SGET_SEED_RATIO_LIMIT));
        imp.seed_ratio_limited.set(protocol::get_bool(&session, protocol::SGET_SEED_RATIO_LIMITED));
        *imp.session.borrow_mut() = Some(session);
        self.emit_by_name::<()>("session-updated", &[]);

        // a new download directory (or the first session): its free space
        // now rather than with the next session update
        if let (true, Some(dir)) = (needs_free_space, dir) {
            let client = self.clone();
            self.dispatch(protocol::free_space(&dir), move |r| {
                let Ok(r) = r else { return };
                {
                    let mut session = client.imp().session.borrow_mut();
                    let Some(s) = session.as_mut() else { return };
                    if protocol::get_str(s, protocol::SGET_DOWNLOAD_DIR) != Some(dir.as_str()) {
                        return;
                    }
                    s.insert(
                        protocol::SGET_DOWNLOAD_DIR_FREE_SPACE.into(),
                        json!(protocol::free_space_bytes(&r)),
                    );
                }
                client.emit_by_name::<()>("session-updated", &[]);
            });
        }
    }

    pub fn version(&self) -> f64 {
        self.imp().version.get()
    }

    pub fn version_string(&self) -> Option<String> {
        self.session()
            .and_then(|s| protocol::get_str(&s, protocol::SGET_VERSION).map(str::to_string))
    }

    /// The server's rpc_version_semver
    pub fn rpc_version(&self) -> Option<String> {
        self.session()
            .and_then(|s| {
                protocol::get_str(&s, protocol::SGET_RPC_VERSION_SEMVER).map(str::to_string)
            })
            .or_else(|| self.imp().transport.rpc_version())
    }

    pub fn is_connected(&self) -> bool {
        self.imp().session.borrow().is_some()
    }

    pub fn inc_connid(&self) {
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

    pub fn fail_count(&self) -> u32 {
        self.imp().fail_count.get()
    }

    pub fn inc_fail_count(&self) -> u32 {
        let c = &self.imp().fail_count;
        c.set(c.get() + 1);
        c.get()
    }

    pub fn reset_fail_count(&self) {
        self.imp().fail_count.set(0);
    }

    pub fn seed_ratio_limited(&self) -> bool {
        self.imp().seed_ratio_limited.get()
    }

    pub fn seed_ratio_limit(&self) -> f64 {
        self.imp().seed_ratio_limit.get()
    }

    pub fn ssl(&self) -> bool {
        self.imp().ssl.get()
    }

    pub fn set_torrent_model(&self, model: &TorrentModel) {
        *self.imp().torrent_model.borrow_mut() = Some(model.clone());
    }

    pub fn torrent_model(&self) -> Option<TorrentModel> {
        self.imp().torrent_model.borrow().clone()
    }

    /// Looks up a torrent by its ID: its JSON object (from the last update)
    /// and item.
    pub fn torrent_data(&self, id: i64) -> Option<(Rc<Obj>, TorrentItem)> {
        let item = self.torrent_model()?.find(id)?;
        Some((item.torrent_json()?, item))
    }

    pub fn status_change(&self, connected: bool) {
        if !connected {
            *self.imp().session.borrow_mut() = None;
            self.prefs().set_connection(None);
        }
    }

    /// Reads the connection settings of the current profile, which becomes
    /// the connection.
    pub fn parse_settings(&self) -> Result<(), String> {
        let prefs = self.prefs();
        prefs.set_connection(prefs.profile());
        let transport = &self.imp().transport;
        transport.configure(None);

        let port = prefs.get_int(prefs::KEY_PORT, prefs::CONNECTION);
        let host = prefs.get_string(prefs::KEY_HOSTNAME, prefs::CONNECTION).unwrap_or_default();
        let path = prefs.get_string(prefs::KEY_RPC_URL_PATH, prefs::CONNECTION).unwrap_or_default();

        if host.is_empty() {
            return Err("Bad hostname.".into());
        }

        let ssl = prefs.get_bool(prefs::KEY_SSL, prefs::CONNECTION);
        self.imp().ssl.set(ssl);

        let url = format!("{}://{host}:{port}{path}", if ssl { "https" } else { "http" });
        if let Err(e) = glib::Uri::parse(&url, glib::UriFlags::NONE) {
            return Err(e.message().to_string());
        }

        let mut headers = Vec::new();
        if let Some(custom) = prefs.get_array(prefs::KEY_CUSTOM_HEADERS, prefs::CONNECTION) {
            for h in protocol::objects(&custom) {
                if let (Some(k), Some(v)) = (
                    protocol::get_str(h, prefs::KEY_CUSTOM_HEADER_NAME),
                    protocol::get_str(h, prefs::KEY_CUSTOM_HEADER_VALUE),
                ) {
                    headers.push((k.to_string(), v.to_string()));
                }
            }
        }

        transport.configure(Some(Config {
            url,
            username: prefs.get_string(prefs::KEY_USERNAME, prefs::CONNECTION),
            password: prefs.get_string(prefs::KEY_PASSWORD, prefs::CONNECTION),
            headers,
            ssl_validate: prefs.get_bool(prefs::KEY_SSL_VALIDATE, prefs::CONNECTION),
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

    /// Sends requests whose results are of no interest, as notifications
    pub fn notify(&self, reqs: Vec<Request>) {
        let client = self.clone();
        glib::spawn_future_local(async move {
            if let Err(e) = client.imp().transport.notify(reqs).await {
                glib::g_message!("trg", "notification failed: {e}");
            }
        });
    }

    /// Sends requests, then gets the session (and the free space of its
    /// download directory) in the same batch. callback gets the results of
    /// the requests and the session.
    pub fn update_session_after<F>(&self, mut reqs: Vec<Request>, callback: F)
    where
        F: FnOnce(Vec<Response>, Response) + 'static,
    {
        let n = reqs.len();
        reqs.push(protocol::session_get());
        let dir = self.download_dir();
        if let Some(dir) = &dir {
            reqs.push(protocol::free_space(dir));
        }
        self.dispatch_batch(reqs, move |mut results| {
            let free_space = if dir.is_some() { results.pop() } else { None };
            let mut session = results.pop().unwrap_or_else(|| Err(Error::Decode(String::new())));
            if let (Ok(s), Some(Ok(free))) = (&mut session, free_space) {
                // only if the directory didn't change in the meantime
                if protocol::get_str(s, protocol::SGET_DOWNLOAD_DIR) == dir.as_deref() {
                    s.insert(
                        protocol::SGET_DOWNLOAD_DIR_FREE_SPACE.into(),
                        json!(protocol::free_space_bytes(&free)),
                    );
                }
            }
            results.truncate(n);
            callback(results, session);
        });
    }

    pub fn update_session<F: FnOnce(Response) + 'static>(&self, callback: F) {
        self.update_session_after(Vec::new(), |_, session| callback(session));
    }
}
