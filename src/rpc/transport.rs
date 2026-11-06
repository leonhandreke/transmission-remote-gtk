//! Sending requests over HTTP with libsoup, on the glib main loop: the
//! session ID handshake, basic authentication, custom headers and TLS
//! certificate validation.

use std::cell::{Cell, RefCell};

use gtk::glib;
use soup::prelude::*;

use super::{
    Error, JSONRPC_MIN_MAJOR, RPC_VERSION_HEADER, Request, Result, SESSION_ID_HEADER,
    SUPPORTED_MAJOR, decode_jsonrpc, encode_jsonrpc, parse_semver,
};

/// The 409 handshakes for one request, after which it fails
const MAX_SESSION_ID_TRIES: u32 = 5;

/// Where and how to connect
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub headers: Vec<(String, String)>,
    pub ssl_validate: bool,
}

pub struct Transport {
    soup: soup::Session,
    config: RefCell<Option<Config>>,
    session_id: RefCell<Option<String>>,
    /// The rpc_version_semver of the X-Transmission-Rpc-Version header
    rpc_version: RefCell<Option<String>>,
    next_id: Cell<u64>,
}

impl Transport {
    pub fn new(user_agent: &str) -> Self {
        let soup = soup::Session::builder().user_agent(user_agent).build();
        if std::env::var_os("TRG_CLIENT_DEBUG").is_some() {
            soup.add_feature(&soup::Logger::new(soup::LoggerLogLevel::Body));
        }
        Self {
            soup,
            config: Default::default(),
            session_id: Default::default(),
            rpc_version: Default::default(),
            next_id: Cell::new(1),
        }
    }

    /// Connects to another server (or the same one again)
    pub fn configure(&self, config: Option<Config>) {
        *self.config.borrow_mut() = config;
        *self.session_id.borrow_mut() = None;
        *self.rpc_version.borrow_mut() = None;
    }

    pub fn abort(&self) {
        self.soup.abort();
    }

    /// The server's rpc_version_semver, from the handshake
    pub fn rpc_version(&self) -> Option<String> {
        self.rpc_version.borrow().clone()
    }

    fn new_message(&self, body: &glib::Bytes) -> Result<soup::Message> {
        let config = self.config.borrow();
        let config = config.as_ref().ok_or_else(|| Error::Transport("Not configured.".into()))?;
        let msg =
            soup::Message::new("POST", &config.url).map_err(|e| Error::Transport(e.to_string()))?;

        if let Some(headers) = msg.request_headers() {
            for (k, v) in &config.headers {
                headers.replace(k, v);
            }
            if let Some(id) = self.session_id.borrow().as_deref() {
                headers.replace(SESSION_ID_HEADER, id);
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

    /// The handshake: the session ID and RPC version of the 409 response
    fn handshake(&self, headers: &soup::MessageHeaders) -> Result<()> {
        // if there's no session ID header this likely isn't Transmission
        let id = headers.one(SESSION_ID_HEADER).ok_or(Error::NoSessionId)?;
        *self.session_id.borrow_mut() = Some(id.to_string());

        // sent from 4.1 on, along with JSON-RPC 2.0
        let version = headers.one(RPC_VERSION_HEADER).map(|s| s.to_string());
        let major = version.as_deref().and_then(parse_semver).map(|v| v.0);
        *self.rpc_version.borrow_mut() = version;
        match major {
            Some(m) if m >= JSONRPC_MIN_MAJOR => {
                if m > SUPPORTED_MAJOR {
                    glib::g_warning!(
                        "trg",
                        "The server's RPC version {m} is newer than the supported one ({SUPPORTED_MAJOR})"
                    );
                }
                Ok(())
            }
            _ => Err(Error::Unsupported),
        }
    }

    /// POSTs a body, after the session ID handshake if needed. Returns the
    /// status and the body of the response.
    async fn post(&self, body: Vec<u8>) -> Result<(u16, glib::Bytes)> {
        let body = glib::Bytes::from_owned(body);
        for _ in 0..MAX_SESSION_ID_TRIES {
            let msg = self.new_message(&body)?;
            let bytes = self
                .soup
                .send_and_read_future(&msg, glib::Priority::DEFAULT)
                .await
                .map_err(|e| Error::Transport(e.message().to_string()))?;
            let status = msg.status_code() as u16;

            // a new session ID, send it again
            if status == 409 {
                self.handshake(&msg.response_headers().ok_or(Error::NoSessionId)?)?;
                continue;
            }

            return Ok((status, bytes));
        }
        Err(Error::Status(409))
    }

    pub async fn call(&self, req: Request) -> Result {
        self.batch(vec![req]).await.pop().unwrap_or_else(|| Err(Error::Decode(String::new())))
    }

    /// Sends the requests in one go, which the server executes in order.
    /// Returns their results, in the same order.
    pub async fn batch(&self, requests: Vec<Request>) -> Vec<Result> {
        let n = requests.len();
        if n == 0 {
            return Vec::new();
        }
        let first_id = self.next_id.get();
        self.next_id.set(first_id + n as u64);

        let result = match self.post(encode_jsonrpc(&requests, Some(first_id))).await {
            Ok((200, bytes)) => decode_jsonrpc(&bytes, first_id, n),
            Ok((status, _)) => Err(Error::Status(status)),
            Err(e) => Err(e),
        };
        result.unwrap_or_else(|e| (0..n).map(|_| Err(e.clone())).collect())
    }

    /// Sends the requests as notifications: the server doesn't respond to
    /// them, for when the result doesn't matter.
    pub async fn notify(&self, requests: Vec<Request>) -> Result<()> {
        if requests.is_empty() {
            return Ok(());
        }
        match self.post(encode_jsonrpc(&requests, None)).await? {
            (200 | 204, _) => Ok(()),
            (status, _) => Err(Error::Status(status)),
        }
    }
}
