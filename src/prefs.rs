//! The configuration, in a JSON file (~/.config/transmission-remote-gtk/
//! config.json), as in the C version (which replaced GConf with this).
//!
//! The values are looked up in one of the profiles (a JSON object in the
//! "profiles" array), the connection (the profile connected with), or the
//! root object, depending on the flags, and then in the defaults. Setting a
//! value emits "pref-changed" with its key.

use std::cell::RefCell;
use std::path::PathBuf;

use gtk::glib;
use gtk::glib::prelude::*;
use gtk::glib::subclass::prelude::*;
use serde_json::{Map, Value, json};

use crate::i18n::tr;

pub const FILENAME: &str = "config.json";
pub const PORT_DEFAULT: i64 = 9091;
pub const INTERVAL_DEFAULT: i64 = 3;
pub const SESSION_INTERVAL_DEFAULT: i64 = 60;
pub const PROFILE_NAME_DEFAULT: &str = "Default";

pub const KEY_RPC_URL_PATH: &str = "rpc-url-path";
pub const KEY_PROFILE_ID: &str = "profile-id";
pub const KEY_PROFILES: &str = "profiles";
pub const KEY_PROFILE_NAME: &str = "profile-name";
pub const KEY_HOSTNAME: &str = "hostname";
pub const KEY_PORT: &str = "port";
pub const KEY_USERNAME: &str = "username";
pub const KEY_PASSWORD: &str = "password";
pub const KEY_CUSTOM_HEADERS: &str = "custom-headers";
pub const KEY_CUSTOM_HEADER_NAME: &str = "custom-header-name";
pub const KEY_CUSTOM_HEADER_VALUE: &str = "custom-header-value";
pub const KEY_AUTO_CONNECT: &str = "auto-connect";
pub const KEY_SSL: &str = "ssl";
pub const KEY_SSL_VALIDATE: &str = "ssl-validate";
pub const KEY_TIMEOUT: &str = "timeout";
pub const KEY_RETRIES: &str = "retries";
pub const KEY_UPDATE_INTERVAL: &str = "update-interval";
pub const KEY_SESSION_UPDATE_INTERVAL: &str = "session-update-interval";
pub const KEY_COMPLETE_NOTIFY: &str = "complete-notify";
pub const KEY_ADD_NOTIFY: &str = "add-notify";
pub const KEY_WINDOW_WIDTH: &str = "window-width";
pub const KEY_WINDOW_HEIGHT: &str = "window-height";
pub const KEY_SYSTEM_TRAY: &str = "system-tray";
pub const KEY_FILTER_TRACKERS: &str = "filter-trackers";
pub const KEY_DIRECTORIES_FIRST: &str = "directories-first";
pub const KEY_FILTER_DIRS: &str = "filter-dirs";
pub const KEY_SHOW_STATE_SELECTOR: &str = "show-state-selector";
pub const KEY_SHOW_NOTEBOOK: &str = "show-notebook";
pub const KEY_LAST_TORRENT_DIR: &str = "last-torrent-dir";
pub const KEY_ADD_OPTIONS_DIALOG: &str = "add-options-dialog";
pub const KEY_START_PAUSED: &str = "start-paused";
pub const KEY_UPDATE_ACTIVE_ONLY: &str = "update-active-only";
pub const KEY_DELETE_LOCAL_TORRENT: &str = "delete-local-torrent";
pub const STATE_SELECTOR_LAST: &str = "state-selector-last";
pub const ACTIVEONLY_FULLSYNC_ENABLED: &str = "activeonly-fullsync-enabled";
pub const ACTIVEONLY_FULLSYNC_EVERY: &str = "activeonly-fullsync-every";
pub const KEY_STYLE: &str = "style";
pub const KEY_TREE_VIEWS: &str = "tree-views";
pub const KEY_TV_SORT_TYPE: &str = "sort-type";
pub const KEY_TV_SORT_COL: &str = "sort-col";
pub const KEY_TV_COLUMNS: &str = "columns";
pub const KEY_TV_WIDTHS: &str = "widths";
pub const KEY_NOTEBOOK_PANED_POS: &str = "notebook-paned-pos";
pub const KEY_STATES_PANED_POS: &str = "states-paned-pos";
pub const SUBKEY_LABEL: &str = "label";
pub const KEY_DESTINATIONS: &str = "destinations";
pub const KEY_LAST_MOVE_DESTINATION: &str = "last-move-destination";
pub const KEY_LAST_ADD_DESTINATION: &str = "last-add-destination";
pub const KEY_DESTINATIONS_SUBKEY_DIR: &str = "dir";
pub const KEY_MINIMIZE_TO_TRAY: &str = "minimize-to-tray";

pub const NOFLAGS: u32 = 1 << 0;
pub const GLOBAL: u32 = 1 << 1;
pub const PROFILE: u32 = 1 << 2;
pub const CONNECTION: u32 = 1 << 3;

pub const STYLE_TR: i64 = 0;
pub const STYLE_CLASSIC: i64 = 1;
pub const STYLE_TR_COMPACT: i64 = 2;

/// Where a value is looked up (before the defaults)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scope {
    Root,
    Profile(usize),
}

/// Integer value of a JSON node, like json_node_get_int()
pub fn value_as_i64(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0),
        Value::Bool(b) => *b as i64,
        _ => 0,
    }
}

/// Floating point value of a JSON node, like json_node_get_double()
pub fn value_as_f64(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::Bool(b) => *b as i64 as f64,
        _ => 0.0,
    }
}

pub fn value_as_bool(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => false,
    }
}

#[derive(Default)]
pub struct State {
    defaults: Map<String, Value>,
    pub user: Value,
    profile: Option<usize>,
    connection: Option<usize>,
    file: PathBuf,
}

impl State {
    fn profiles(&self) -> &[Value] {
        self.user.get(KEY_PROFILES).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
    }

    fn profiles_mut(&mut self) -> &mut Vec<Value> {
        let root = self.root_mut();
        if !root.get(KEY_PROFILES).is_some_and(Value::is_array) {
            root.insert(KEY_PROFILES.into(), json!([]));
        }
        root.get_mut(KEY_PROFILES).and_then(Value::as_array_mut).unwrap()
    }

    pub fn root_mut(&mut self) -> &mut Map<String, Value> {
        if !self.user.is_object() {
            self.user = json!({});
        }
        self.user.as_object_mut().unwrap()
    }

    fn scope(&self, flags: u32) -> Scope {
        if let (Some(p), true) = (self.profile, flags & PROFILE != 0) {
            Scope::Profile(p)
        } else if let (Some(c), true) = (self.connection, flags & CONNECTION != 0) {
            Scope::Profile(c)
        } else {
            Scope::Root
        }
    }

    fn object(&self, scope: Scope) -> Option<&Map<String, Value>> {
        match scope {
            Scope::Root => self.user.as_object(),
            Scope::Profile(i) => self.profiles().get(i).and_then(Value::as_object),
        }
    }

    fn object_mut(&mut self, scope: Scope) -> Option<&mut Map<String, Value>> {
        match scope {
            Scope::Root => Some(self.root_mut()),
            Scope::Profile(i) => self.profiles_mut().get_mut(i).and_then(Value::as_object_mut),
        }
    }

    fn get(&self, key: &str, flags: u32) -> Option<Value> {
        if let Some(v) = self.object(self.scope(flags)).and_then(|o| o.get(key)) {
            return Some(v.clone());
        }
        self.defaults.get(key).cloned()
    }

    fn set(&mut self, key: &str, value: Value, flags: u32) {
        let scope = self.scope(flags);
        if let Some(obj) = self.object_mut(scope) {
            obj.insert(key.to_string(), value);
        }
    }

    fn empty_init(&mut self) {
        self.user = json!({ KEY_PROFILES: [{}], KEY_PROFILE_ID: 0 });
        self.profile = Some(0);
    }
}

mod imp {
    use super::*;
    use std::sync::OnceLock;

    #[derive(Default)]
    pub struct Prefs {
        pub state: RefCell<State>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Prefs {
        const NAME: &'static str = "TrgPrefs";
        type Type = super::Prefs;
    }

    impl ObjectImpl for Prefs {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // the key, or None for many (the profile changed)
                    glib::subclass::Signal::builder("pref-changed")
                        .param_types([Option::<String>::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("pref-profile-changed").build(),
                ]
            })
        }
    }
}

glib::wrapper! {
    pub struct Prefs(ObjectSubclass<imp::Prefs>);
}

impl Default for Prefs {
    fn default() -> Self {
        Self::new()
    }
}

impl Prefs {
    pub fn new() -> Self {
        let prefs: Self = glib::Object::new();
        {
            let mut st = prefs.imp().state.borrow_mut();
            let d = &mut st.defaults;
            d.insert(KEY_PROFILE_NAME.into(), json!(tr(PROFILE_NAME_DEFAULT)));
            d.insert(KEY_RPC_URL_PATH.into(), json!("/transmission/rpc"));
            d.insert(KEY_PORT.into(), json!(PORT_DEFAULT));
            d.insert(KEY_UPDATE_INTERVAL.into(), json!(INTERVAL_DEFAULT));
            d.insert(KEY_SESSION_UPDATE_INTERVAL.into(), json!(SESSION_INTERVAL_DEFAULT));
            d.insert(ACTIVEONLY_FULLSYNC_EVERY.into(), json!(2));
            d.insert(KEY_TIMEOUT.into(), json!(40));
            d.insert(KEY_RETRIES.into(), json!(3));
            for key in [
                KEY_FILTER_DIRS,
                KEY_FILTER_TRACKERS,
                KEY_DIRECTORIES_FIRST,
                KEY_ADD_OPTIONS_DIALOG,
                KEY_SHOW_STATE_SELECTOR,
                KEY_MINIMIZE_TO_TRAY,
            ] {
                d.insert(key.into(), json!(true));
            }
            // Not the application name, which is translated and for display
            st.file = glib::user_config_dir().join(env!("TRG_PACKAGE_NAME")).join(FILENAME);
        }
        prefs
    }

    pub fn state(&self) -> std::cell::Ref<'_, State> {
        self.imp().state.borrow()
    }

    pub fn state_mut(&self) -> std::cell::RefMut<'_, State> {
        self.imp().state.borrow_mut()
    }

    pub fn emit_changed(&self, key: Option<&str>) {
        self.emit_by_name::<()>("pref-changed", &[&key.map(str::to_string)]);
    }

    pub fn emit_profile_changed(&self) {
        self.emit_by_name::<()>("pref-profile-changed", &[]);
    }

    pub fn connect_changed<F: Fn(&Self, Option<&str>) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("pref-changed", false, move |args| {
            let prefs = args[0].get::<Self>().unwrap();
            let key = args[1].get::<Option<String>>().unwrap();
            f(&prefs, key.as_deref());
            None
        })
    }

    pub fn connect_profile_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("pref-profile-changed", false, move |args| {
            f(&args[0].get::<Self>().unwrap());
            None
        })
    }

    pub fn get_value(&self, key: &str, flags: u32) -> Option<Value> {
        self.state().get(key, flags)
    }

    pub fn get_string(&self, key: &str, flags: u32) -> Option<String> {
        self.get_value(key, flags).and_then(|v| v.as_str().map(str::to_string))
    }

    pub fn get_int(&self, key: &str, flags: u32) -> i64 {
        self.get_value(key, flags).map(|v| value_as_i64(&v)).unwrap_or(0)
    }

    pub fn get_double(&self, key: &str, flags: u32) -> f64 {
        self.get_value(key, flags).map(|v| value_as_f64(&v)).unwrap_or(0.0)
    }

    pub fn get_bool(&self, key: &str, flags: u32) -> bool {
        self.get_value(key, flags).map(|v| value_as_bool(&v)).unwrap_or(false)
    }

    pub fn get_array(&self, key: &str, flags: u32) -> Option<Vec<Value>> {
        self.get_value(key, flags).and_then(|v| v.as_array().cloned())
    }

    /// Sets a value without emitting "pref-changed"
    pub fn set_value_quiet(&self, key: &str, value: Value, flags: u32) {
        self.state_mut().set(key, value, flags);
    }

    pub fn set_value(&self, key: &str, value: Value, flags: u32) {
        self.set_value_quiet(key, value, flags);
        self.emit_changed(Some(key));
    }

    pub fn set_int(&self, key: &str, value: i64, flags: u32) {
        self.set_value(key, json!(value), flags);
    }

    pub fn set_string(&self, key: &str, value: &str, flags: u32) {
        self.set_value(key, json!(value), flags);
    }

    pub fn set_double(&self, key: &str, value: f64, flags: u32) {
        self.set_value(key, json!(value), flags);
    }

    pub fn set_bool(&self, key: &str, value: bool, flags: u32) {
        self.set_value(key, json!(value), flags);
    }

    /// Removes a value from the scope of flags
    pub fn remove(&self, key: &str, flags: u32) {
        let mut st = self.state_mut();
        let scope = st.scope(flags);
        if let Some(obj) = st.object_mut(scope) {
            obj.remove(key);
        }
    }

    pub fn profile_id(&self) -> i64 {
        self.state().user.get(KEY_PROFILE_ID).map(value_as_i64).unwrap_or(0)
    }

    pub fn n_profiles(&self) -> usize {
        self.state().profiles().len()
    }

    pub fn profiles(&self) -> Vec<Value> {
        self.state().profiles().to_vec()
    }

    /// The index of the current profile
    pub fn profile(&self) -> Option<usize> {
        self.state().profile
    }

    pub fn connection(&self) -> Option<usize> {
        self.state().connection
    }

    pub fn set_connection(&self, profile: Option<usize>) {
        self.state_mut().connection = profile;
    }

    /// Makes a profile (or none) the current one
    pub fn set_profile(&self, index: Option<usize>) {
        let set_id = {
            let mut st = self.state_mut();
            st.profile = index.filter(|i| *i < st.profiles().len());
            if let Some(i) = st.profile {
                st.root_mut().insert(KEY_PROFILE_ID.into(), json!(i));
            }
            st.profile.is_some()
        };
        if set_id {
            self.emit_changed(Some(KEY_PROFILE_ID));
        }
        self.emit_changed(None);
        self.emit_profile_changed();
    }

    /// Adds a new (empty) profile, returns its index
    pub fn new_profile(&self) -> usize {
        let mut st = self.state_mut();
        let profiles = st.profiles_mut();
        profiles.push(json!({}));
        profiles.len() - 1
    }

    pub fn del_profile(&self, index: usize) {
        {
            let mut st = self.state_mut();
            let profiles = st.profiles_mut();
            if index < profiles.len() {
                profiles.remove(index);
            }
            let fix = |p: Option<usize>| match p {
                Some(p) if p == index => None,
                Some(p) if p > index => Some(p - 1),
                p => p,
            };
            st.profile = fix(st.profile);
            st.connection = fix(st.connection);
        }
        self.emit_profile_changed();
    }

    /// A profile's object (a copy)
    pub fn profile_object(&self, index: usize) -> Option<Map<String, Value>> {
        self.state().profiles().get(index).and_then(Value::as_object).cloned()
    }

    pub fn with_profile_mut<R>(
        &self,
        index: usize,
        f: impl FnOnce(&mut Map<String, Value>) -> R,
    ) -> Option<R> {
        let mut st = self.state_mut();
        st.profiles_mut().get_mut(index).and_then(Value::as_object_mut).map(f)
    }

    pub fn with_root_mut<R>(&self, f: impl FnOnce(&mut Map<String, Value>) -> R) -> R {
        f(self.state_mut().root_mut())
    }

    pub fn load(&self) {
        let mut st = self.state_mut();
        let parsed = std::fs::read(&st.file)
            .ok()
            .and_then(|data| serde_json::from_slice::<Value>(&data).ok())
            .filter(Value::is_object);

        let Some(root) = parsed else {
            st.empty_init();
            return;
        };
        st.user = root;

        let n_profiles = st.profiles_mut().len();
        if n_profiles < 1 {
            st.profiles_mut().push(json!({}));
            st.profile = Some(0);
            st.root_mut().insert(KEY_PROFILE_ID.into(), json!(0));
        } else {
            let mut id = st.user.get(KEY_PROFILE_ID).map(value_as_i64).unwrap_or(0);
            if id < 0 || id as usize >= n_profiles {
                id = 0;
                st.root_mut().insert(KEY_PROFILE_ID.into(), json!(0));
            }
            st.profile = Some(id as usize);
        }
    }

    pub fn save(&self) -> bool {
        let st = self.state();
        let file = st.file.clone();
        let dir = file.parent().map(PathBuf::from).unwrap_or_default();
        let mut is_new = true;

        if !dir.is_dir() {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                glib::g_warning!(
                    "trg",
                    "Problem creating parent directory (permissions?) for: {}: {e}",
                    file.display()
                );
                return false;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
            }
        } else if file.is_file() {
            is_new = false;
        }

        let mut data = serde_json::to_string_pretty(&st.user).unwrap_or_default();
        data.push('\n');
        drop(st);

        // Write to the target of a symbolic link, rather than replacing it
        let mut real = file.clone();
        if let Ok(link) = std::fs::read_link(&file) {
            real = if link.is_absolute() { link } else { dir.join(link) };
        }

        let ok = glib::file_set_contents(&real, data.as_bytes()).is_ok();
        if !ok {
            glib::g_warning!(
                "trg",
                "Problem writing configuration file (permissions?) to: {}",
                file.display()
            );
        } else if is_new {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
            }
        }
        ok
    }

    /// The flags for adding torrents, from the preferences
    pub fn add_flags(&self) -> u32 {
        let mut flags = 0;
        if self.get_bool(KEY_START_PAUSED, GLOBAL) {
            flags |= crate::protocol::TORRENT_ADD_FLAG_PAUSED;
        }
        if self.get_bool(KEY_DELETE_LOCAL_TORRENT, GLOBAL) {
            flags |= crate::protocol::TORRENT_ADD_FLAG_DELETE;
        }
        flags
    }

    /// The tree view's settings object in "tree-views", created if needed
    pub fn with_tree_view_props<R>(
        &self,
        config_id: &str,
        f: impl FnOnce(&mut Map<String, Value>) -> R,
    ) -> R {
        self.with_root_mut(|root| {
            if !root.get(KEY_TREE_VIEWS).is_some_and(Value::is_object) {
                root.insert(KEY_TREE_VIEWS.into(), json!({}));
            }
            let tvs = root.get_mut(KEY_TREE_VIEWS).and_then(Value::as_object_mut).unwrap();
            if !tvs.get(config_id).is_some_and(Value::is_object) {
                tvs.insert(config_id.into(), json!({}));
            }
            f(tvs.get_mut(config_id).and_then(Value::as_object_mut).unwrap())
        })
    }
}
