//! The client of a Transmission server, passed around a lot, as in the C
//! version. It:
//!
//! 1. holds the preferences,
//! 2. holds the libsoup session, for connection reuse,
//! 3. finds torrents by their ID (in the torrent model),
//! 4. dispatches asynchronous requests and tracks failures,
//! 5. holds the connection state and an update serial,
//! 6. holds the latest session object of a session-get response, and emits
//!    "session-updated" with it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::glib;
use gtk::glib::prelude::*;
use gtk::glib::subclass::prelude::*;
use serde_json::Value;
use soup::prelude::*;

use crate::i18n::tr;
use crate::prefs::{self, Prefs};
use crate::protocol::{self, Obj, Request};
use crate::torrent_model::{TorrentItem, TorrentItemExt, TorrentModel};

pub const TRANSMISSION_MIN_SUPPORTED: f64 = 2.0;
pub const TRANSMISSION_SESSION_ID_HEADER: &str = "X-Transmission-Session-Id";

pub const STATUS_OK: i32 = 200;
pub const FAIL_HTTP_UNSUCCESSFUL: i32 = -1;
pub const FAIL_JSON_DECODE: i32 = -2;
pub const FAIL_RESULT_UNSUCCESSFUL: i32 = -3;
pub const FAIL_NO_SESSION_ID: i32 = -4;

/// The response to a request: the HTTP status (or one of the FAIL_ codes),
/// an error message and the response object.
#[derive(Debug, Default, Clone)]
pub struct Response {
    pub status: i32,
    pub err_msg: Option<String>,
    pub obj: Option<Obj>,
}

impl Response {
    pub fn ok(&self) -> bool {
        self.status == STATUS_OK
    }

    pub fn arguments(&self) -> Option<&Obj> {
        self.obj.as_ref().and_then(protocol::arguments)
    }
}

/// The message for a failed response
pub fn make_error_message(response: &Response) -> String {
    match response.status {
        FAIL_HTTP_UNSUCCESSFUL => {
            response.err_msg.clone().unwrap_or_else(|| "Unknown HTTP failure.".into())
        }
        FAIL_JSON_DECODE => {
            response.err_msg.clone().unwrap_or_else(|| "Unknown JSON decoding error.".into())
        }
        FAIL_RESULT_UNSUCCESSFUL => response
            .obj
            .as_ref()
            .and_then(|o| protocol::get_str(o, protocol::FIELD_RESULT))
            .map(str::to_string)
            .unwrap_or_else(|| "Server responded, but with no result.".into()),
        FAIL_NO_SESSION_ID => {
            format!("No \"{TRANSMISSION_SESSION_ID_HEADER}\" header sent, is this the right host?")
        }
        status => crate::i18n::sprintf(&tr("Request failed with HTTP code: %d"), &[&status]),
    }
}

#[derive(Default)]
struct Config {
    url: Option<String>,
    username: Option<String>,
    password: Option<String>,
    headers: Vec<(String, String)>,
    ssl: bool,
    ssl_validate: bool,
}

mod imp {
    use super::*;
    use std::sync::OnceLock;

    pub struct Client {
        pub prefs: Prefs,
        pub soup: soup::Session,
        pub session_id: RefCell<Option<String>>,
        pub connid: Cell<i64>,
        pub fail_count: Cell<u32>,
        pub update_serial: Cell<i64>,
        pub session: RefCell<Option<Obj>>,
        pub version: Cell<f64>,
        pub(super) config: RefCell<Config>,
        pub seed_ratio_limited: Cell<bool>,
        pub seed_ratio_limit: Cell<f64>,
        pub torrent_model: RefCell<Option<TorrentModel>>,
    }

    impl Default for Client {
        fn default() -> Self {
            Self {
                prefs: Prefs::new(),
                soup: soup::Session::builder().user_agent(env!("TRG_PACKAGE_NAME")).build(),
                session_id: Default::default(),
                connid: Default::default(),
                fail_count: Default::default(),
                update_serial: Default::default(),
                session: Default::default(),
                version: Default::default(),
                config: Default::default(),
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
            self.soup.abort();
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
        if std::env::var_os("TRG_CLIENT_DEBUG").is_some() {
            let logger = soup::Logger::new(soup::LoggerLogLevel::Body);
            client.imp().soup.add_feature(&logger);
        }
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

    pub fn set_session(&self, session: Obj) {
        let imp = self.imp();
        if imp.session.borrow().is_none() {
            imp.version.set(protocol::session_get_version(Some(&session)));
        }
        imp.seed_ratio_limit.set(protocol::get_f64(&session, protocol::SGET_SEED_RATIO_LIMIT));
        imp.seed_ratio_limited.set(protocol::get_bool(&session, protocol::SGET_SEED_RATIO_LIMITED));
        *imp.session.borrow_mut() = Some(session);
        self.emit_by_name::<()>("session-updated", &[]);
    }

    pub fn version(&self) -> f64 {
        self.imp().version.get()
    }

    pub fn version_string(&self) -> Option<String> {
        self.session()
            .and_then(|s| protocol::get_str(&s, protocol::SGET_VERSION).map(str::to_string))
    }

    pub fn rpc_version(&self) -> i64 {
        self.session().map(|s| protocol::get_i64(&s, protocol::SGET_RPC_VERSION)).unwrap_or(0)
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
        self.imp().config.borrow().ssl
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

        let mut config = Config::default();

        let port = prefs.get_int(prefs::KEY_PORT, prefs::CONNECTION);
        let host = prefs.get_string(prefs::KEY_HOSTNAME, prefs::CONNECTION).unwrap_or_default();
        let path = prefs.get_string(prefs::KEY_RPC_URL_PATH, prefs::CONNECTION).unwrap_or_default();

        if host.is_empty() {
            *self.imp().config.borrow_mut() = config;
            return Err("Bad hostname.".into());
        }

        config.ssl = prefs.get_bool(prefs::KEY_SSL, prefs::CONNECTION);
        config.ssl_validate = prefs.get_bool(prefs::KEY_SSL_VALIDATE, prefs::CONNECTION);

        let url = format!("{}://{host}:{port}{path}", if config.ssl { "https" } else { "http" });
        if let Err(e) = glib::Uri::parse(&url, glib::UriFlags::NONE) {
            *self.imp().config.borrow_mut() = config;
            return Err(e.message().to_string());
        }
        config.url = Some(url);

        config.username = prefs.get_string(prefs::KEY_USERNAME, prefs::CONNECTION);
        config.password = prefs.get_string(prefs::KEY_PASSWORD, prefs::CONNECTION);

        if let Some(headers) = prefs.get_array(prefs::KEY_CUSTOM_HEADERS, prefs::CONNECTION) {
            for h in protocol::objects(&headers) {
                if let (Some(k), Some(v)) = (
                    protocol::get_str(h, prefs::KEY_CUSTOM_HEADER_NAME),
                    protocol::get_str(h, prefs::KEY_CUSTOM_HEADER_VALUE),
                ) {
                    config.headers.push((k.to_string(), v.to_string()));
                }
            }
        }

        *self.imp().config.borrow_mut() = config;
        Ok(())
    }

    fn new_message(&self, body: &glib::Bytes) -> Result<soup::Message, String> {
        let config = self.imp().config.borrow();
        let url = config.url.clone().ok_or_else(|| "Not configured.".to_string())?;
        let msg = soup::Message::new("POST", &url).map_err(|e| e.to_string())?;

        if let Some(headers) = msg.request_headers() {
            for (k, v) in &config.headers {
                headers.replace(k, v);
            }
            if let Some(id) = self.imp().session_id.borrow().as_deref() {
                headers.replace(TRANSMISSION_SESSION_ID_HEADER, id);
            }
        }

        let ssl_validate = config.ssl_validate;
        msg.connect_accept_certificate(move |_, _, _| !ssl_validate);

        let username = config.username.clone().unwrap_or_default();
        let password = config.password.clone().unwrap_or_default();
        let auth_body = body.clone();
        msg.connect_authenticate(move |msg, auth, retrying| {
            if retrying {
                return false;
            }
            auth.authenticate(&username, &password);
            // the body has to be set again when authenticating
            msg.set_request_body_from_bytes(Some("application/json"), Some(&auth_body));
            true
        });

        msg.set_request_body_from_bytes(Some("application/json"), Some(body));
        Ok(msg)
    }

    async fn send(&self, body: glib::Bytes) -> Response {
        let mut tries = 0;
        loop {
            let msg = match self.new_message(&body) {
                Ok(msg) => msg,
                Err(e) => {
                    return Response {
                        status: FAIL_HTTP_UNSUCCESSFUL,
                        err_msg: Some(e),
                        obj: None,
                    };
                }
            };

            let result = self.imp().soup.send_and_read_future(&msg, glib::Priority::DEFAULT).await;

            let bytes = match result {
                Ok(bytes) => bytes,
                Err(e) => {
                    return Response {
                        status: FAIL_HTTP_UNSUCCESSFUL,
                        err_msg: Some(e.message().to_string()),
                        obj: None,
                    };
                }
            };

            let status = msg.status_code() as i32;

            // a new session ID, send the request again with it
            if status == 409 {
                let session_id = msg
                    .response_headers()
                    .and_then(|h| h.one(TRANSMISSION_SESSION_ID_HEADER))
                    .map(|s| s.to_string());
                match session_id {
                    // if there's no session ID header this likely isn't Transmission
                    None => return Response { status: FAIL_NO_SESSION_ID, ..Default::default() },
                    Some(id) => {
                        *self.imp().session_id.borrow_mut() = Some(id);
                        tries += 1;
                        if tries < 5 {
                            continue;
                        }
                    }
                }
            }

            if status != STATUS_OK {
                return Response { status, ..Default::default() };
            }

            return parse_response(&bytes);
        }
    }

    /// Sends a request, calls callback with the response, unless the client
    /// connected or disconnected in the meantime.
    pub fn dispatch<F: FnOnce(Response) + 'static>(&self, req: Request, callback: F) {
        let body = glib::Bytes::from_owned(serde_json::to_vec(&req).unwrap_or_default());
        let connid = self.connid();
        let client = self.clone();
        glib::spawn_future_local(async move {
            let response = client.send(body).await;
            if client.connid() == connid {
                callback(response);
            }
        });
    }

    /// Sends a request whose response is of no interest
    pub fn dispatch_ignore(&self, req: Request) {
        self.dispatch(req, |_| {});
    }

    pub fn update_session<F: FnOnce(Response) + 'static>(&self, callback: F) {
        self.dispatch(protocol::session_get(), callback);
    }
}

fn parse_response(bytes: &[u8]) -> Response {
    // Transmission may send invalid UTF-8 (#261 of the C version)
    let parsed = match std::str::from_utf8(bytes) {
        Ok(s) => serde_json::from_str::<Value>(s),
        Err(_) => {
            glib::g_warning!(
                "trg",
                "Invalid JSON received from Transmission, fixing it, but data may be wrong/corrupted"
            );
            serde_json::from_str::<Value>(&String::from_utf8_lossy(bytes))
        }
    };

    let obj = match parsed {
        Ok(Value::Object(obj)) => obj,
        Ok(_) => return Response { status: FAIL_JSON_DECODE, ..Default::default() },
        Err(e) => {
            return Response { status: FAIL_JSON_DECODE, err_msg: Some(e.to_string()), obj: None };
        }
    };

    let status = if protocol::get_str(&obj, protocol::FIELD_RESULT) == Some(protocol::FIELD_SUCCESS)
    {
        STATUS_OK
    } else {
        FAIL_RESULT_UNSUCCESSFUL
    };

    Response { status, err_msg: None, obj: Some(obj) }
}
