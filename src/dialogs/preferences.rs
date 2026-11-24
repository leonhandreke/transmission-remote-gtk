//! The local preferences: the connection profiles, updates, adding torrents and
//! the view.
//!
//! The widgets of the template which show a preference are listed with the
//! key and scope of the preference, and loaded and saved with them.
//!
//! The view settings (the sidebar, its options, the details) are previewed:
//! their check boxes set the preference straight away, which the window
//! follows, and they follow it (the menu's view items set it too). Closing
//! without OK restores the values they had when the dialog opened.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, gio, glib};
use serde_json::{Value, json};

use crate::client::Client;
use crate::dialogs::dialog::{Dialog, DialogImpl, Response, TrgDialogExt};
use crate::i18n::tr;
use crate::json::{get_str, objects};
use crate::macos;
use crate::prefs::{self, Prefs, Scope};
use crate::tree_view::{
    COLUMN_EDITABLE, COLUMN_UNREMOVABLE, ColType, ColumnDesc, TreeView, row_object,
};
use crate::util;

/* The items of the profile drop down: a GObject with a "name" property,
 * which the template binds */

mod profile_imp {
    use super::*;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::ProfileItem)]
    pub struct ProfileItem {
        #[property(get, set)]
        pub name: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProfileItem {
        const NAME: &'static str = "TrgProfileItem";
        type Type = super::ProfileItem;
    }

    #[glib::derived_properties]
    impl ObjectImpl for ProfileItem {}
}

glib::wrapper! {
    pub struct ProfileItem(ObjectSubclass<profile_imp::ProfileItem>);
}

impl ProfileItem {
    fn new(name: &str) -> Self {
        glib::Object::builder().property("name", name).build()
    }
}

/* An entry of a config list: two texts (like a label and a directory) */

mod row_imp {
    use super::*;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::PersistentRow)]
    pub struct PersistentRow {
        #[property(get, set)]
        pub first: RefCell<String>,
        #[property(get, set)]
        pub second: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PersistentRow {
        const NAME: &'static str = "TrgPersistentRow";
        type Type = super::PersistentRow;
    }

    #[glib::derived_properties]
    impl ObjectImpl for PersistentRow {}
}

glib::wrapper! {
    pub struct PersistentRow(ObjectSubclass<row_imp::PersistentRow>);
}

/* A config list which saves and restores simple data in the preferences: a
 * box with a tree view of editable text columns, and the buttons to add,
 * delete and move entries. */

struct PersistentColumn {
    /// The key of its text in the entries of the preference
    key: String,
    /// The property of the rows it shows
    property: &'static str,
    desc: Rc<ColumnDesc>,
}

pub struct PersistentTreeView {
    pub widget: gtk::Box,
    tv: TreeView,
    store: gio::ListStore,
    columns: RefCell<Vec<PersistentColumn>>,
    del_button: gtk::Button,
    up_button: gtk::Button,
    down_button: gtk::Button,
    add_select: RefCell<Option<usize>>,
    key: String,
    scope: Scope,
}

impl PersistentTreeView {
    fn new(key: &str, scope: Scope) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let store = gio::ListStore::new::<PersistentRow>();

        // no prefs: nothing about the view itself is persisted
        let tv = TreeView::new(None, "TrgPersistentTreeView");
        tv.set_sortable(false);
        tv.set_column_menus(false);
        tv.set_model(&store);
        tv.set_vexpand(true);
        tv.set_margin_top(4);
        tv.set_margin_bottom(4);
        widget.append(&tv);

        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let button = |label: &str, sensitive: bool| {
            let b = gtk::Button::with_label(label);
            b.set_sensitive(sensitive);
            b.set_margin_start(4);
            b.set_margin_end(4);
            hbox.append(&b);
            b
        };
        let add_button = button(&tr("Add"), true);
        let del_button = button(&tr("Delete"), false);
        let up_button = button(&tr("Up"), false);
        let down_button = button(&tr("Down"), false);
        hbox.set_margin_top(4);
        hbox.set_margin_bottom(4);
        widget.append(&hbox);

        let ptv = Rc::new(Self {
            widget,
            tv: tv.clone(),
            store: store.clone(),
            columns: Default::default(),
            del_button: del_button.clone(),
            up_button: up_button.clone(),
            down_button: down_button.clone(),
            add_select: Default::default(),
            key: key.to_string(),
            scope,
        });

        let weak = Rc::downgrade(&ptv);
        tv.selection().connect_selection_changed(move |_, _, _| {
            if let Some(p) = weak.upgrade() {
                p.update_buttons();
            }
        });
        // removing selected entries doesn't change the selection
        let weak = Rc::downgrade(&ptv);
        tv.selection().connect_items_changed(move |_, _, _, _| {
            if let Some(p) = weak.upgrade() {
                p.update_buttons();
            }
        });
        // Enter or a double click on a row edits it
        let weak = Rc::downgrade(&ptv);
        tv.connect_row_activated(move |tv, position| {
            let Some(p) = weak.upgrade() else { return };
            if let Some(row) =
                tv.selection().item(position).and_then(row_object).and_downcast::<PersistentRow>()
            {
                p.edit_row(&row);
            }
        });

        let weak = Rc::downgrade(&ptv);
        add_button.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                let row: PersistentRow = glib::Object::new();
                p.store.append(&row);
                p.edit_row(&row);
            }
        });
        let weak = Rc::downgrade(&ptv);
        del_button.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                for row in p.tv.selected_items::<PersistentRow>() {
                    if let Some(pos) = p.store.find(&row) {
                        p.store.remove(pos);
                    }
                }
            }
        });
        let weak = Rc::downgrade(&ptv);
        up_button.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                p.move_selected(-1);
            }
        });
        let weak = Rc::downgrade(&ptv);
        down_button.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                p.move_selected(1);
            }
        });

        ptv
    }

    /// The position of the only selected entry in the store
    fn selected_position(&self) -> Option<u32> {
        if self.tv.count_selected() != 1 {
            return None;
        }
        self.tv.first_selected::<PersistentRow>().and_then(|r| self.store.find(&r))
    }

    fn update_buttons(&self) {
        let position = self.selected_position();
        let n = self.store.n_items();
        self.del_button.set_sensitive(self.tv.count_selected() > 0);
        self.up_button.set_sensitive(position.is_some_and(|p| p > 0));
        self.down_button.set_sensitive(position.is_some_and(|p| p + 1 < n));
    }

    /// Adds a column of the texts with key, shown in the property of the rows
    fn add_column(self: &Rc<Self>, property: &'static str, key: &str, label: &str) -> usize {
        // nothing is saved, the sort key is the column's position
        let position = self.columns.borrow().len() as i32;
        let desc = self.tv.reg_property(
            ColType::Text,
            position,
            property,
            label,
            key,
            COLUMN_UNREMOVABLE | COLUMN_EDITABLE,
        );
        desc.set_edit(move |row: &PersistentRow, done, new_text| {
            if let (true, Some(text)) = (done, new_text) {
                row.set_property(property, text);
            }
        });
        self.columns.borrow_mut().push(PersistentColumn { key: key.to_string(), property, desc });
        self.tv.remove_all_columns();
        self.tv.setup_columns();
        self.columns.borrow().len() - 1
    }

    fn set_add_select(&self, column: usize) {
        *self.add_select.borrow_mut() = Some(column);
    }

    fn edit_row(&self, row: &PersistentRow) {
        let columns = self.columns.borrow();
        let column = self.add_select.borrow().and_then(|i| columns.get(i)).or(columns.first());
        self.tv.select_item(row);
        if let Some(c) = column {
            self.tv.start_editing(row, &c.desc);
        }
    }

    fn move_selected(&self, offset: i32) {
        let Some(position) = self.selected_position() else { return };
        let n = self.store.n_items();
        if (offset < 0 && position == 0) || (offset > 0 && position + 1 >= n) {
            return;
        }
        let row = self.store.item(position).and_downcast::<PersistentRow>().unwrap();
        self.store.remove(position);
        self.store.insert((position as i32 + offset) as u32, &row);
        self.tv.select_item(&row);
    }

    fn refresh(&self, prefs: &Prefs) {
        let entries = prefs.get_array(&self.key, self.scope).unwrap_or_default();
        let columns = self.columns.borrow();
        let rows: Vec<PersistentRow> = objects(&entries)
            .map(|obj| {
                let row: PersistentRow = glib::Object::new();
                for c in columns.iter() {
                    row.set_property(c.property, get_str(obj, &c.key).unwrap_or_default());
                }
                row
            })
            .collect();
        self.store.splice(0, self.store.n_items(), &rows);
    }

    fn save(&self, prefs: &Prefs) {
        // keep an entry that is still being edited, like when OK is clicked
        self.tv.stop_editing();
        let columns = self.columns.borrow();
        let entries: Vec<Value> = util::objects::<PersistentRow>(&self.store)
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for c in columns.iter() {
                    obj.insert(c.key.clone(), json!(row.property::<String>(c.property)));
                }
                Value::Object(obj)
            })
            .collect();
        prefs.set_value(&self.key, Value::Array(entries), self.scope);
    }
}

/// A widget showing a preference
pub enum PrefWidget {
    Simple { widget: gtk::Widget, key: &'static str, scope: Scope },
    List(Rc<PersistentTreeView>),
}

impl PrefWidget {
    fn refresh(&self, prefs: &Prefs) {
        match self {
            PrefWidget::Simple { widget, key, scope } => {
                if let Some(check) = widget.downcast_ref::<gtk::CheckButton>() {
                    check.set_active(prefs.get_bool(key, *scope));
                } else if let Some(spin) = widget.downcast_ref::<gtk::SpinButton>() {
                    spin.set_value(prefs.get_int(key, *scope) as f64);
                } else if let Some(e) = widget.dynamic_cast_ref::<gtk::Editable>() {
                    e.set_text(&prefs.get_string(key, *scope).unwrap_or_default());
                }
            }
            PrefWidget::List(ptv) => ptv.refresh(prefs),
        }
    }

    fn save(&self, prefs: &Prefs) {
        match self {
            PrefWidget::Simple { widget, key, scope } => {
                if let Some(check) = widget.downcast_ref::<gtk::CheckButton>() {
                    prefs.set_bool(key, check.is_active(), *scope);
                } else if let Some(spin) = widget.downcast_ref::<gtk::SpinButton>() {
                    prefs.set_int(key, spin.value_as_int() as i64, *scope);
                } else if let Some(e) = widget.dynamic_cast_ref::<gtk::Editable>() {
                    prefs.set_string(key, &e.text(), *scope);
                }
            }
            PrefWidget::List(ptv) => ptv.save(prefs),
        }
    }
}

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/preferences-dialog.ui")]
    pub struct PreferencesDialog {
        #[template_child]
        pub notebook: TemplateChild<gtk::Notebook>,
        #[template_child]
        pub connection_scroll: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub profile_del_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub profile_combo: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub profile_store: TemplateChild<gio::ListStore>,
        #[template_child]
        pub full_update_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub full_update_spin: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub headers_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub destinations_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub profile_name_entry: TemplateChild<gtk::Widget>,
        #[template_child]
        pub hostname_entry: TemplateChild<gtk::Widget>,
        #[template_child]
        pub port_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub rpc_url_path_entry: TemplateChild<gtk::Widget>,
        #[template_child]
        pub username_entry: TemplateChild<gtk::Widget>,
        #[template_child]
        pub password_entry: TemplateChild<gtk::Widget>,
        #[template_child]
        pub auto_connect_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub ssl_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub ssl_validate_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub timeout_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub retries_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub update_active_only_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub update_interval_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub session_update_interval_spin: TemplateChild<gtk::Widget>,
        #[template_child]
        pub start_paused_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub add_options_dialog_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub delete_local_torrent_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub show_state_selector_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub filter_dirs_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub filter_trackers_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub directories_first_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub show_notebook_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub add_notify_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub complete_notify_check: TemplateChild<gtk::Widget>,

        pub client: RefCell<Option<Client>>,
        pub widgets: RefCell<Vec<PrefWidget>>,
        /// The view settings when the dialog opened, restored unless it's
        /// closed with OK
        pub view_before: RefCell<Vec<(&'static str, bool)>>,
        /// Its check boxes following the view settings
        pub view_handlers: RefCell<Vec<glib::SignalHandlerId>>,
        pub populating: std::cell::Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesDialog {
        const NAME: &'static str = "TrgPreferencesDialog";
        type Type = super::PreferencesDialog;
        type ParentType = Dialog;

        fn class_init(klass: &mut Self::Class) {
            ProfileItem::ensure_type();
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PreferencesDialog {}
    impl WidgetImpl for PreferencesDialog {}
    impl WindowImpl for PreferencesDialog {}
    impl DialogImpl for PreferencesDialog {}
}

glib::wrapper! {
    pub struct PreferencesDialog(ObjectSubclass<imp::PreferencesDialog>)
        @extends Dialog, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root,
            gtk::ShortcutManager;
}

fn dlg(w: &impl IsA<gtk::Widget>) -> Option<PreferencesDialog> {
    util::ancestor::<PreferencesDialog>(w)
}

#[gtk::template_callbacks]
impl PreferencesDialog {
    #[template_callback]
    fn profile_changed_cb(combo: &gtk::DropDown, _pspec: &glib::ParamSpec) {
        let Some(d) = dlg(combo) else { return };
        let imp = d.imp();
        // Selecting the current profile isn't a change
        if imp.populating.get() {
            return;
        }
        d.save_all();
        let prefs = d.prefs();
        if combo.selected_item().is_some() {
            prefs.set_profile(Some(combo.selected() as usize));
            d.refresh_all();
            imp.profile_del_button.set_sensitive(imp.profile_store.n_items() > 1);
        } else {
            imp.profile_del_button.set_sensitive(false);
            combo.set_selected(0);
        }
    }

    #[template_callback]
    fn name_changed_cb(entry: &gtk::Entry) {
        let Some(d) = dlg(entry) else { return };
        if let Some(item) = d.imp().profile_combo.selected_item().and_downcast::<ProfileItem>() {
            item.set_name(entry.text().to_string());
        }
    }

    #[template_callback]
    fn add_profile_cb(button: &gtk::Button) {
        let Some(d) = dlg(button) else { return };
        let imp = d.imp();
        d.prefs().new_profile();
        imp.profile_store.append(&ProfileItem::new(&tr(prefs::PROFILE_NAME_DEFAULT)));
        imp.profile_combo.set_selected(imp.profile_store.n_items() - 1);
    }

    #[template_callback]
    fn del_profile_cb(button: &gtk::Button) {
        let Some(d) = dlg(button) else { return };
        let imp = d.imp();
        let combo = &imp.profile_combo;
        let position = combo.selected();
        if combo.selected_item().is_some() {
            let prefs = d.prefs();
            prefs.del_profile(position as usize);
            prefs.set_profile(None);
            imp.profile_store.remove(position);
            combo.set_selected(0);
        }
    }

    /// The full update interval only matters when updating active torrents
    /// only and doing full updates
    #[template_callback]
    fn full_update_dependency_cb(check: &gtk::CheckButton, _pspec: &glib::ParamSpec) {
        if let Some(d) = dlg(check) {
            d.full_update_dependency();
        }
    }

    pub fn new(win: &impl IsA<gtk::Window>, client: &Client) -> Self {
        let d: Self = glib::Object::new();
        let imp = d.imp();
        *imp.client.borrow_mut() = Some(client.clone());

        d.set_transient_for(Some(win));
        if util::is_macos() {
            d.macos_settings_window(win.upcast_ref());
        }
        d.profile_combo_populate();

        let simple: Vec<(gtk::Widget, &'static str, Scope)> = vec![
            (imp.profile_name_entry.get().upcast(), prefs::KEY_PROFILE_NAME, Scope::Profile),
            (imp.hostname_entry.get().upcast(), prefs::KEY_HOSTNAME, Scope::Profile),
            (imp.port_spin.get().upcast(), prefs::KEY_PORT, Scope::Profile),
            (imp.rpc_url_path_entry.get().upcast(), prefs::KEY_RPC_URL_PATH, Scope::Profile),
            (imp.username_entry.get().upcast(), prefs::KEY_USERNAME, Scope::Profile),
            (imp.password_entry.get().upcast(), prefs::KEY_PASSWORD, Scope::Profile),
            (imp.auto_connect_check.get().upcast(), prefs::KEY_AUTO_CONNECT, Scope::Profile),
            (imp.ssl_check.get().upcast(), prefs::KEY_SSL, Scope::Profile),
            (imp.ssl_validate_check.get().upcast(), prefs::KEY_SSL_VALIDATE, Scope::Profile),
            (imp.timeout_spin.get().upcast(), prefs::KEY_TIMEOUT, Scope::Profile),
            (imp.retries_spin.get().upcast(), prefs::KEY_RETRIES, Scope::Profile),
            (
                imp.update_active_only_check.get().upcast(),
                prefs::KEY_UPDATE_ACTIVE_ONLY,
                Scope::Profile,
            ),
            (
                imp.full_update_check.get().upcast(),
                prefs::ACTIVEONLY_FULLSYNC_ENABLED,
                Scope::Profile,
            ),
            (imp.full_update_spin.get().upcast(), prefs::ACTIVEONLY_FULLSYNC_EVERY, Scope::Profile),
            (imp.update_interval_spin.get().upcast(), prefs::KEY_UPDATE_INTERVAL, Scope::Profile),
            (
                imp.session_update_interval_spin.get().upcast(),
                prefs::KEY_SESSION_UPDATE_INTERVAL,
                Scope::Profile,
            ),
            (imp.start_paused_check.get().upcast(), prefs::KEY_START_PAUSED, Scope::Global),
            (
                imp.add_options_dialog_check.get().upcast(),
                prefs::KEY_ADD_OPTIONS_DIALOG,
                Scope::Global,
            ),
            (
                imp.delete_local_torrent_check.get().upcast(),
                prefs::KEY_DELETE_LOCAL_TORRENT,
                Scope::Global,
            ),
            (
                imp.show_state_selector_check.get().upcast(),
                prefs::KEY_SHOW_STATE_SELECTOR,
                Scope::Global,
            ),
            (imp.filter_dirs_check.get().upcast(), prefs::KEY_FILTER_DIRS, Scope::Global),
            (imp.filter_trackers_check.get().upcast(), prefs::KEY_FILTER_TRACKERS, Scope::Global),
            (
                imp.directories_first_check.get().upcast(),
                prefs::KEY_DIRECTORIES_FIRST,
                Scope::Global,
            ),
            (imp.show_notebook_check.get().upcast(), prefs::KEY_SHOW_NOTEBOOK, Scope::Global),
            (imp.add_notify_check.get().upcast(), prefs::KEY_ADD_NOTIFY, Scope::Global),
            (imp.complete_notify_check.get().upcast(), prefs::KEY_COMPLETE_NOTIFY, Scope::Global),
        ];
        for (widget, key, scope) in simple {
            let wd = PrefWidget::Simple { widget, key, scope };
            wd.refresh(&d.prefs());
            imp.widgets.borrow_mut().push(wd);
        }

        d.add_persistent_list(
            &imp.headers_box,
            prefs::KEY_CUSTOM_HEADERS,
            Scope::Profile,
            (prefs::KEY_CUSTOM_HEADER_NAME, &tr("Name")),
            (prefs::KEY_CUSTOM_HEADER_VALUE, &tr("Value")),
        );
        d.add_persistent_list(
            &imp.destinations_box,
            prefs::KEY_DESTINATIONS,
            Scope::Connection,
            (prefs::SUBKEY_LABEL, &tr("Label")),
            (prefs::KEY_DESTINATIONS_SUBKEY_DIR, &tr("Directory")),
        );

        d.full_update_dependency();
        d.preview_view_prefs();

        d.on_response(|d, response| {
            let prefs = d.prefs();
            for id in d.imp().view_handlers.take() {
                prefs.disconnect(id);
            }
            if response == Response::Ok {
                d.save_all();
                prefs.save();
            } else {
                for (key, value) in d.imp().view_before.take() {
                    if prefs.get_bool(key, Scope::Global) != value {
                        prefs.set_bool(key, value, Scope::Global);
                    }
                }
            }
            if let Some(client) = d.imp().client.borrow().as_ref() {
                client.reload_dir_aliases();
            }
            d.imp().widgets.borrow_mut().clear();
            d.destroy();
        });

        d
    }

    fn prefs(&self) -> Prefs {
        self.imp().client.borrow().as_ref().unwrap().prefs().clone()
    }

    pub fn set_page(&self, page: u32) {
        self.imp().notebook.set_current_page(Some(page));
    }

    fn refresh_all(&self) {
        let prefs = self.prefs();
        for wd in self.imp().widgets.borrow().iter() {
            wd.refresh(&prefs);
        }
    }

    fn save_all(&self) {
        let prefs = self.prefs();
        if prefs.profile().is_none() {
            return;
        }
        for wd in self.imp().widgets.borrow().iter() {
            wd.save(&prefs);
        }
    }

    /// The view settings' check boxes set and follow their preferences (see
    /// the top)
    fn preview_view_prefs(&self) {
        let imp = self.imp();
        let prefs = self.prefs();
        let checks = [
            (&imp.show_state_selector_check, prefs::KEY_SHOW_STATE_SELECTOR),
            (&imp.filter_dirs_check, prefs::KEY_FILTER_DIRS),
            (&imp.filter_trackers_check, prefs::KEY_FILTER_TRACKERS),
            (&imp.directories_first_check, prefs::KEY_DIRECTORIES_FIRST),
            (&imp.show_notebook_check, prefs::KEY_SHOW_NOTEBOOK),
        ];
        for (check, key) in checks {
            imp.view_before.borrow_mut().push((key, prefs.get_bool(key, Scope::Global)));
            let p = prefs.clone();
            check.connect_toggled(move |check| {
                // not when it follows the preference
                if p.get_bool(key, Scope::Global) != check.is_active() {
                    p.set_bool(key, check.is_active(), Scope::Global);
                }
            });
            let weak = check.downgrade();
            let id = prefs.connect_changed(move |prefs, changed| {
                if let (Some(check), true) = (weak.upgrade(), changed == Some(key)) {
                    check.set_active(prefs.get_bool(key, Scope::Global));
                }
            });
            imp.view_handlers.borrow_mut().push(id);
        }
    }

    fn full_update_dependency(&self) {
        let imp = self.imp();
        let check = &imp.full_update_check;
        imp.full_update_spin.set_sensitive(check.is_sensitive() && check.is_active());
    }

    /// This is the application's Settings window on macOS, which has a fixed
    /// size and its minimize and zoom buttons dimmed. It's sized by its
    /// content then, so the Connection tab (which is the tallest, and
    /// scrolls) has to ask for its full height, as far as the screen allows.
    fn macos_settings_window(&self, win: &gtk::Window) {
        let scroll = &self.imp().connection_scroll;
        scroll.set_propagate_natural_height(true);
        if let Some(monitor) = win.surface().and_then(|s| s.display().monitor_at_surface(&s)) {
            // leaving room for the menu bar, the Dock and the rest of the window
            scroll.set_max_content_height((monitor.geometry().height() - 300).max(300));
        }
        self.set_resizable(false);
        self.connect_realize(macos::make_fixed);
    }

    fn profile_combo_populate(&self) {
        let imp = self.imp();
        let prefs = self.prefs();
        let profile_id = prefs.get_int(prefs::KEY_PROFILE_ID, Scope::Global);
        let profiles = prefs.profiles();

        imp.populating.set(true);
        for profile in &profiles {
            let name = profile
                .get(prefs::KEY_PROFILE_NAME)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| tr(prefs::PROFILE_NAME_DEFAULT));
            imp.profile_store.append(&ProfileItem::new(&name));
        }
        if profile_id >= 0 && (profile_id as u32) < imp.profile_store.n_items() {
            imp.profile_combo.set_selected(profile_id as u32);
        }
        imp.populating.set(false);

        imp.profile_del_button.set_sensitive(profiles.len() > 1);
    }

    /// A list of label/value pairs (the remote download directories, the
    /// custom headers), in a box of the template
    fn add_persistent_list(
        &self,
        bx: &gtk::Box,
        key: &str,
        scope: Scope,
        col1: (&str, &str),
        col2: (&str, &str),
    ) {
        let ptv = PersistentTreeView::new(key, scope);
        let first = ptv.add_column("first", col1.0, col1.1);
        ptv.set_add_select(first);
        ptv.add_column("second", col2.0, col2.1);
        let wd = PrefWidget::List(ptv.clone());
        wd.refresh(&self.prefs());
        self.imp().widgets.borrow_mut().push(wd);
        ptv.widget.set_vexpand(true);
        bx.append(&ptv.widget);
    }
}
