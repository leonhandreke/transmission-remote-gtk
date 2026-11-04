//! The local preferences: the connection profiles, updates, adding torrents
//! and the view (trg-preferences-dialog.c and trg-persistent-tree-view.c).
//!
//! The widgets of the template which show a preference are listed with the
//! key and flags of the preference, and loaded and saved with them.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, gio, glib};
use serde_json::{Value, json};

use crate::client::Client;
use crate::dialog::{self, Dialog, DialogImpl, TrgDialogExt};
use crate::i18n::tr;
use crate::item::{self, Item, Val};
use crate::main_window::MainWindow;
use crate::prefs::{self, Prefs};
use crate::protocol::{self as p, get_str};
use crate::tree_view::{COLUMN_EDITABLE, COLUMN_UNREMOVABLE, ColType, ColumnDesc, TreeView};

thread_local! {
    static INSTANCE: RefCell<glib::WeakRef<PreferencesDialog>> = RefCell::new(glib::WeakRef::new());
}

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

/* A config list which saves and restores simple data in the preferences: a
 * box with a tree view of editable text columns, and the buttons to add,
 * delete and move entries. */

struct PersistentColumn {
    key: String,
    index: usize,
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
    flags: u32,
}

fn new_persistent_item() -> Item {
    Item::new(vec![Val::Str(None), Val::Str(None)])
}

impl PersistentTreeView {
    fn new(key: &str, flags: u32) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let store = gio::ListStore::new::<Item>();

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
            flags,
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
            if let Some(item) = tv.selection().item(position).and_then(|r| item::row_item(&r)) {
                p.edit_item(&item);
            }
        });

        let weak = Rc::downgrade(&ptv);
        add_button.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                let item = new_persistent_item();
                p.store.append(&item);
                p.edit_item(&item);
            }
        });
        let weak = Rc::downgrade(&ptv);
        del_button.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                for item in p.tv.selected_items() {
                    if let Some(pos) = p.store.find(&item) {
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
        self.tv.first_selected().and_then(|i| self.store.find(&i))
    }

    fn update_buttons(&self) {
        let position = self.selected_position();
        let n = self.store.n_items();
        self.del_button.set_sensitive(self.tv.count_selected() > 0);
        self.up_button.set_sensitive(position.is_some_and(|p| p > 0));
        self.down_button.set_sensitive(position.is_some_and(|p| p + 1 < n));
    }

    fn add_column(self: &Rc<Self>, index: usize, key: &str, label: &str) -> usize {
        let desc = self.tv.reg_column(
            ColType::Text,
            index as i32,
            label,
            key,
            COLUMN_UNREMOVABLE | COLUMN_EDITABLE,
        );
        desc.set_edit(move |_, item, done, new_text| {
            if let (true, Some(text)) = (done, new_text) {
                item.set(index, text);
            }
        });
        self.columns.borrow_mut().push(PersistentColumn { key: key.to_string(), index, desc });
        self.tv.remove_all_columns();
        self.tv.setup_columns();
        self.columns.borrow().len() - 1
    }

    fn set_add_select(&self, column: usize) {
        *self.add_select.borrow_mut() = Some(column);
    }

    fn edit_item(&self, item: &Item) {
        let columns = self.columns.borrow();
        let column = self.add_select.borrow().and_then(|i| columns.get(i)).or(columns.first());
        self.tv.select_item(item);
        if let Some(c) = column {
            self.tv.start_editing(item, &c.desc);
        }
    }

    fn move_selected(&self, offset: i32) {
        let Some(position) = self.selected_position() else { return };
        let n = self.store.n_items();
        if (offset < 0 && position == 0) || (offset > 0 && position + 1 >= n) {
            return;
        }
        let item = self.store.item(position).and_downcast::<Item>().unwrap();
        self.store.remove(position);
        self.store.insert((position as i32 + offset) as u32, &item);
        self.tv.select_item(&item);
    }

    fn refresh(&self, prefs: &Prefs) {
        let entries = prefs.get_array(&self.key, self.flags).unwrap_or_default();
        let columns = self.columns.borrow();
        let items: Vec<Item> = p::objects(&entries)
            .map(|obj| {
                let item = new_persistent_item();
                for c in columns.iter() {
                    item.set(c.index, get_str(obj, &c.key));
                }
                item
            })
            .collect();
        self.store.splice(0, self.store.n_items(), &items);
    }

    fn save(&self, prefs: &Prefs) {
        // keep an entry that is still being edited, like when OK is clicked
        self.tv.stop_editing();
        let columns = self.columns.borrow();
        let entries: Vec<Value> = item::items(&self.store)
            .map(|item| {
                let mut obj = serde_json::Map::new();
                for c in columns.iter() {
                    obj.insert(c.key.clone(), json!(item.string(c.index).unwrap_or_default()));
                }
                Value::Object(obj)
            })
            .collect();
        prefs.set_value(&self.key, Value::Array(entries), self.flags);
    }
}

/// A widget showing a preference
pub enum PrefWidget {
    Simple { widget: gtk::Widget, key: &'static str, flags: u32 },
    List(Rc<PersistentTreeView>),
}

impl PrefWidget {
    fn refresh(&self, prefs: &Prefs) {
        match self {
            PrefWidget::Simple { widget, key, flags } => {
                if let Some(check) = widget.downcast_ref::<gtk::CheckButton>() {
                    check.set_active(prefs.get_bool(key, *flags));
                } else if let Some(spin) = widget.downcast_ref::<gtk::SpinButton>() {
                    spin.set_value(prefs.get_int(key, *flags) as f64);
                } else if let Some(e) = widget.dynamic_cast_ref::<gtk::Editable>() {
                    e.set_text(&prefs.get_string(key, *flags).unwrap_or_default());
                }
            }
            PrefWidget::List(ptv) => ptv.refresh(prefs),
        }
    }

    fn save(&self, prefs: &Prefs) {
        match self {
            PrefWidget::Simple { widget, key, flags } => {
                if let Some(check) = widget.downcast_ref::<gtk::CheckButton>() {
                    prefs.set_bool(key, check.is_active(), *flags);
                } else if let Some(spin) = widget.downcast_ref::<gtk::SpinButton>() {
                    prefs.set_int(key, spin.value_as_int() as i64, *flags);
                } else if let Some(e) = widget.dynamic_cast_ref::<gtk::Editable>() {
                    prefs.set_string(key, &e.text(), *flags);
                }
            }
            PrefWidget::List(ptv) => ptv.save(prefs),
        }
    }
}

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-preferences-dialog.ui")]
    pub struct PreferencesDialog {
        #[template_child]
        pub notebook: TemplateChild<gtk::Notebook>,
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
        pub show_state_selector_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub filter_dirs_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub filter_trackers_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub directories_first_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub show_notebook_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub system_tray_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub minimize_to_tray_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub add_notify_check: TemplateChild<gtk::Widget>,
        #[template_child]
        pub complete_notify_check: TemplateChild<gtk::Widget>,

        pub client: RefCell<Option<Client>>,
        pub win: glib::WeakRef<MainWindow>,
        pub widgets: RefCell<Vec<PrefWidget>>,
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

pub fn instance(win: &MainWindow, client: &Client) -> PreferencesDialog {
    if let Some(d) = INSTANCE.with(|i| i.borrow().upgrade()) {
        return d;
    }
    let d = PreferencesDialog::new(win, client);
    INSTANCE.with(|i| i.borrow().set(Some(&d)));
    d
}

fn dlg(w: &impl IsA<gtk::Widget>) -> Option<PreferencesDialog> {
    dialog::ancestor::<PreferencesDialog>(w)
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

    #[template_callback]
    fn view_states_toggled_cb(check: &gtk::CheckButton) {
        if let Some(w) = dlg(check).and_then(|d| d.imp().win.upgrade()) {
            w.state_selector().set_visible(check.is_active());
        }
    }

    #[template_callback]
    fn filter_dirs_toggled_cb(check: &gtk::CheckButton) {
        if let Some(w) = dlg(check).and_then(|d| d.imp().win.upgrade()) {
            w.state_selector().set_show_dirs(check.is_active());
        }
    }

    #[template_callback]
    fn filter_trackers_toggled_cb(check: &gtk::CheckButton) {
        if let Some(w) = dlg(check).and_then(|d| d.imp().win.upgrade()) {
            w.state_selector().set_show_trackers(check.is_active());
        }
    }

    #[template_callback]
    fn directories_first_toggled_cb(check: &gtk::CheckButton) {
        if let Some(w) = dlg(check).and_then(|d| d.imp().win.upgrade()) {
            w.state_selector().set_directories_first(check.is_active());
        }
    }

    #[template_callback]
    fn notebook_toggled_cb(check: &gtk::CheckButton) {
        if let Some(w) = dlg(check).and_then(|d| d.imp().win.upgrade()) {
            w.notebook_set_visible(check.is_active());
        }
    }

    fn new(win: &MainWindow, client: &Client) -> Self {
        let d: Self = glib::Object::new();
        let imp = d.imp();
        *imp.client.borrow_mut() = Some(client.clone());
        imp.win.set(Some(win));

        d.set_transient_for(Some(win));
        d.profile_combo_populate();

        let simple: Vec<(gtk::Widget, &'static str, u32)> = vec![
            (imp.profile_name_entry.get().upcast(), prefs::KEY_PROFILE_NAME, prefs::PROFILE),
            (imp.hostname_entry.get().upcast(), prefs::KEY_HOSTNAME, prefs::PROFILE),
            (imp.port_spin.get().upcast(), prefs::KEY_PORT, prefs::PROFILE),
            (imp.rpc_url_path_entry.get().upcast(), prefs::KEY_RPC_URL_PATH, prefs::PROFILE),
            (imp.username_entry.get().upcast(), prefs::KEY_USERNAME, prefs::PROFILE),
            (imp.password_entry.get().upcast(), prefs::KEY_PASSWORD, prefs::PROFILE),
            (imp.auto_connect_check.get().upcast(), prefs::KEY_AUTO_CONNECT, prefs::PROFILE),
            (imp.ssl_check.get().upcast(), prefs::KEY_SSL, prefs::PROFILE),
            (imp.ssl_validate_check.get().upcast(), prefs::KEY_SSL_VALIDATE, prefs::PROFILE),
            (imp.timeout_spin.get().upcast(), prefs::KEY_TIMEOUT, prefs::PROFILE),
            (imp.retries_spin.get().upcast(), prefs::KEY_RETRIES, prefs::PROFILE),
            (
                imp.update_active_only_check.get().upcast(),
                prefs::KEY_UPDATE_ACTIVE_ONLY,
                prefs::PROFILE,
            ),
            (
                imp.full_update_check.get().upcast(),
                prefs::ACTIVEONLY_FULLSYNC_ENABLED,
                prefs::PROFILE,
            ),
            (imp.full_update_spin.get().upcast(), prefs::ACTIVEONLY_FULLSYNC_EVERY, prefs::PROFILE),
            (imp.update_interval_spin.get().upcast(), prefs::KEY_UPDATE_INTERVAL, prefs::PROFILE),
            (
                imp.session_update_interval_spin.get().upcast(),
                prefs::KEY_SESSION_UPDATE_INTERVAL,
                prefs::PROFILE,
            ),
            (imp.start_paused_check.get().upcast(), prefs::KEY_START_PAUSED, prefs::GLOBAL),
            (
                imp.add_options_dialog_check.get().upcast(),
                prefs::KEY_ADD_OPTIONS_DIALOG,
                prefs::GLOBAL,
            ),
            (
                imp.delete_local_torrent_check.get().upcast(),
                prefs::KEY_DELETE_LOCAL_TORRENT,
                prefs::GLOBAL,
            ),
            (
                imp.show_state_selector_check.get().upcast(),
                prefs::KEY_SHOW_STATE_SELECTOR,
                prefs::GLOBAL,
            ),
            (imp.filter_dirs_check.get().upcast(), prefs::KEY_FILTER_DIRS, prefs::GLOBAL),
            (imp.filter_trackers_check.get().upcast(), prefs::KEY_FILTER_TRACKERS, prefs::GLOBAL),
            (
                imp.directories_first_check.get().upcast(),
                prefs::KEY_DIRECTORIES_FIRST,
                prefs::GLOBAL,
            ),
            (imp.show_notebook_check.get().upcast(), prefs::KEY_SHOW_NOTEBOOK, prefs::GLOBAL),
            (imp.system_tray_check.get().upcast(), prefs::KEY_SYSTEM_TRAY, prefs::GLOBAL),
            (imp.minimize_to_tray_check.get().upcast(), prefs::KEY_MINIMIZE_TO_TRAY, prefs::GLOBAL),
            (imp.add_notify_check.get().upcast(), prefs::KEY_ADD_NOTIFY, prefs::GLOBAL),
            (imp.complete_notify_check.get().upcast(), prefs::KEY_COMPLETE_NOTIFY, prefs::GLOBAL),
        ];
        for (widget, key, flags) in simple {
            let wd = PrefWidget::Simple { widget, key, flags };
            wd.refresh(&d.prefs());
            imp.widgets.borrow_mut().push(wd);
        }

        d.add_persistent_list(
            &imp.headers_box,
            prefs::KEY_CUSTOM_HEADERS,
            prefs::PROFILE,
            (prefs::KEY_CUSTOM_HEADER_NAME, &tr("Name")),
            (prefs::KEY_CUSTOM_HEADER_VALUE, &tr("Value")),
        );
        d.add_persistent_list(
            &imp.destinations_box,
            prefs::KEY_DESTINATIONS,
            prefs::CONNECTION,
            (prefs::SUBKEY_LABEL, &tr("Label")),
            (prefs::KEY_DESTINATIONS_SUBKEY_DIR, &tr("Directory")),
        );

        d.full_update_dependency();

        d.on_response(|d, response| {
            let prefs = d.prefs();
            if response == dialog::RESPONSE_OK {
                d.save_all();
                prefs.save();
            }
            if let Some(win) = d.imp().win.upgrade() {
                win.reload_dir_aliases();
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

    fn full_update_dependency(&self) {
        let imp = self.imp();
        let check = &imp.full_update_check;
        imp.full_update_spin.set_sensitive(check.is_sensitive() && check.is_active());
    }

    fn profile_combo_populate(&self) {
        let imp = self.imp();
        let prefs = self.prefs();
        let profile_id = prefs.get_int(prefs::KEY_PROFILE_ID, prefs::GLOBAL);
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
        flags: u32,
        col1: (&str, &str),
        col2: (&str, &str),
    ) {
        let ptv = PersistentTreeView::new(key, flags);
        let first = ptv.add_column(0, col1.0, col1.1);
        ptv.set_add_select(first);
        ptv.add_column(1, col2.0, col2.1);
        let wd = PrefWidget::List(ptv.clone());
        wd.refresh(&self.prefs());
        self.imp().widgets.borrow_mut().push(wd);
        ptv.widget.set_vexpand(true);
        bx.append(&ptv.widget);
    }
}
