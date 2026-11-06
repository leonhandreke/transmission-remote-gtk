//! The files of a torrent as a tree (trg-files-model.c, trg-files-tree.c,
//! trg-files-tree-view.c and trg-file-rename-dialog.c). A directory (ID -1)
//! has its children in a list store; its columns sum up (size, bytes
//! completed, progress) or combine (wanted, priority: TR_PRI_MIXED if its
//! files differ) those of its children.
//!
//! The view is used for the files of the selected torrent, and (locally) for
//! the files of a torrent to add.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, gio, glib};
use serde_json::Value;

use crate::client::Client;
use crate::dialog::{self, Dialog, DialogImpl, TrgDialogExt};
use crate::i18n::{sprintf, tr};
use crate::item::{self, Item, Val};
use crate::main_window::MainWindow;
use crate::prefs::{self, Prefs};
use crate::protocol::{
    self as p, Obj, TR_PRI_HIGH, TR_PRI_LOW, TR_PRI_MIXED, TR_PRI_NORMAL, get_i64, get_str,
};
use crate::tree_view::{ColType, TreeView};
use crate::util;

pub const COL_NAME: usize = 0;
pub const COL_SIZE: usize = 1;
pub const COL_PROGRESS: usize = 2;
pub const COL_ID: usize = 3;
pub const COL_WANTED: usize = 4;
pub const COL_PRIORITY: usize = 5;
pub const COL_BYTESCOMPLETED: usize = 6;

/// Build the tree in a thread for torrents with more files
const CREATE_THREAD_IF_GT: usize = 600;

fn new_file_item() -> Item {
    Item::new(vec![
        Val::Str(None),
        Val::I64(0),
        Val::F64(0.0),
        Val::I64(0),
        Val::I64(0),
        Val::I64(0),
        Val::I64(0),
    ])
}

/// A node of a tree of files, built with simple data structures, which is
/// quicker than going straight to the model, and can be done in a thread.
#[derive(Default, Debug)]
pub struct FileNode {
    pub name: Option<String>,
    pub length: i64,
    pub bytes_completed: i64,
    pub children: Vec<FileNode>,
    children_by_name: HashMap<String, usize>,
    pub index: i64,
    pub priority: i64,
    pub enabled: i64,
}

impl FileNode {
    /// The top (nameless) node
    pub fn top() -> Self {
        Self { index: -1, ..Default::default() }
    }

    /// A single file (the top of a torrent in single file mode)
    pub fn file(name: &str, length: i64, index: i64) -> Self {
        Self { name: Some(name.to_string()), length, index, enabled: 1, ..Default::default() }
    }

    fn child(&mut self, name: &str, is_file: bool) -> &mut FileNode {
        let existing = if is_file { None } else { self.children_by_name.get(name).copied() };
        let i = match existing {
            Some(i) => i,
            None => {
                self.children.push(FileNode {
                    name: Some(name.to_string()),
                    index: -1,
                    ..Default::default()
                });
                let i = self.children.len() - 1;
                self.children_by_name.insert(name.to_string(), i);
                i
            }
        };
        &mut self.children[i]
    }

    /// Inserts a file under its path ("dir/sub/file")
    pub fn insert(
        &mut self,
        path: &str,
        length: i64,
        bytes_completed: i64,
        index: i64,
        enabled: i64,
        priority: i64,
    ) {
        let parts: Vec<&str> = path.split('/').collect();
        let mut node = self;
        for (i, part) in parts.iter().enumerate() {
            let is_file = i == parts.len() - 1;
            node = node.child(part, is_file);
            if is_file {
                node.length = length;
                node.bytes_completed = bytes_completed;
                node.index = index;
                node.enabled = enabled;
                node.priority = priority;
            }
        }
    }

    /// The tree of the files of a torrent-get response
    fn from_torrent(files: &[Value], wanted: &[Value], priorities: &[Value]) -> Self {
        let mut top = Self::top();
        for (i, file) in files.iter().enumerate() {
            let Some(file) = file.as_object() else { continue };
            top.insert(
                get_str(file, p::TFILE_NAME).unwrap_or(""),
                get_i64(file, p::TFILE_LENGTH),
                get_i64(file, p::TFILE_BYTES_COMPLETED),
                i as i64,
                wanted.get(i).map(prefs::value_as_i64).unwrap_or(0),
                priorities.get(i).map(prefs::value_as_i64).unwrap_or(0),
            );
        }
        top
    }
}

/// The path of a file item, from the top
pub fn file_item_path(item: &Item) -> String {
    let mut path = item.string(COL_NAME).unwrap_or_default();
    let mut parent = item.parent_item();
    while let Some(p) = parent {
        path = format!("{}/{path}", p.string(COL_NAME).unwrap_or_default());
        parent = p.parent_item();
    }
    path
}

/// Sums up or combines the columns of a directory's children
fn update_dir(dir: &Item) {
    let Some(children) = dir.children() else { return };
    let (mut size, mut completed) = (0, 0);
    let (mut wanted, mut priority) = (1, TR_PRI_NORMAL);

    for (i, child) in item::items(&children).enumerate() {
        let cw = child.int64(COL_WANTED);
        let cp = child.int64(COL_PRIORITY);
        size += child.int64(COL_SIZE);
        completed += child.int64(COL_BYTESCOMPLETED);
        if i == 0 {
            wanted = cw;
            priority = cp;
        } else {
            if wanted != cw {
                wanted = TR_PRI_MIXED;
            }
            if priority != cp {
                priority = TR_PRI_MIXED;
            }
        }
    }

    dir.set(COL_SIZE, size);
    dir.set(COL_BYTESCOMPLETED, completed);
    dir.set(COL_PROGRESS, p::file_get_progress(size, completed));
    dir.set(COL_WANTED, wanted);
    dir.set(COL_PRIORITY, priority);
}

/// Updates the directories of a subtree, the deepest first
fn update_tree(item: &Item) {
    let Some(children) = item.children() else { return };
    for child in item::items(&children) {
        update_tree(&child);
    }
    update_dir(item);
}

fn set_subtree(item: &Item, column: usize, value: i64) {
    if let Some(children) = item.children() {
        for child in item::items(&children) {
            set_subtree(&child, column, value);
        }
    }
    item.set(column, value);
}

fn set_column(item: &Item, column: usize, value: i64) {
    set_subtree(item, column, value);
    let mut parent = item.parent_item();
    while let Some(p) = parent {
        update_dir(&p);
        parent = p.parent_item();
    }
}

pub fn set_wanted(item: &Item, wanted: i64) {
    set_column(item, COL_WANTED, wanted);
}

pub fn set_priority(item: &Item, priority: i64) {
    set_column(item, COL_PRIORITY, priority);
}

pub struct FilesModel {
    pub store: gio::ListStore,
    torrent_id: Cell<i64>,
    accept: Cell<bool>,
    /// the file items by file index
    files: RefCell<Vec<Option<glib::WeakRef<Item>>>>,
    tree_model: RefCell<Option<gtk::TreeListModel>>,
    /// a tree being built in a thread
    build_id: Cell<u32>,
    building: Cell<bool>,
    rebuilt: RefCell<Vec<Box<dyn Fn()>>>,
}

impl FilesModel {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            store: gio::ListStore::new::<Item>(),
            torrent_id: Cell::new(-1),
            accept: Cell::new(true),
            files: Default::default(),
            tree_model: Default::default(),
            build_id: Cell::new(0),
            building: Cell::new(false),
            rebuilt: Default::default(),
        })
    }

    pub fn connect_rebuilt(&self, f: impl Fn() + 'static) {
        self.rebuilt.borrow_mut().push(Box::new(f));
    }

    fn item_new(&self, parent: Option<&Item>, node: &FileNode, defaults: bool) -> Item {
        let item = new_file_item();
        item.set_parent_item(parent);
        item.set(COL_NAME, node.name.clone());
        item.set(COL_ID, if node.children.is_empty() { node.index } else { -1 });

        if !node.children.is_empty() {
            let children: Vec<Item> =
                node.children.iter().map(|c| self.item_new(Some(&item), c, defaults)).collect();
            let store = gio::ListStore::new::<Item>();
            store.splice(0, 0, &children);
            item.set_children(Some(store));
        } else {
            item.set(COL_SIZE, node.length);
            if defaults {
                item.set(COL_WANTED, 1i64);
                item.set(COL_PRIORITY, TR_PRI_NORMAL);
            } else {
                item.set(COL_BYTESCOMPLETED, node.bytes_completed);
                item.set(COL_PROGRESS, p::file_get_progress(node.length, node.bytes_completed));
                item.set(COL_WANTED, node.enabled);
                item.set(COL_PRIORITY, node.priority);
            }
            if node.index >= 0 {
                let mut files = self.files.borrow_mut();
                let index = node.index as usize;
                if index >= files.len() {
                    files.resize(index + 1, None);
                }
                files[index] = Some(item.downgrade());
            }
        }
        item
    }

    fn remove_all(&self) {
        self.build_id.set(self.build_id.get() + 1);
        self.building.set(false);
        self.files.borrow_mut().clear();
        self.store.remove_all();
    }

    pub fn clear(&self) {
        self.remove_all();
        self.torrent_id.set(-1);
    }

    /// Replaces the items with the tree under top (a nameless node, or a
    /// single file)
    fn set_nodes(&self, top: &FileNode, defaults: bool) {
        self.remove_all();
        let items: Vec<Item> = if top.name.is_some() {
            vec![self.item_new(None, top, defaults)]
        } else {
            top.children.iter().map(|c| self.item_new(None, c, defaults)).collect()
        };
        for item in &items {
            update_tree(item);
        }
        // the top level in one go
        self.store.splice(0, 0, &items);
        for f in self.rebuilt.borrow().iter() {
            f();
        }
    }

    /// Shows a tree of local files (of a torrent to add), all wanted.
    /// Returns the number of files.
    pub fn set_tree(&self, top: &FileNode) -> usize {
        self.set_nodes(top, true);
        self.accept.set(true);
        self.files.borrow().iter().filter(|f| f.is_some()).count()
    }

    pub fn set_accept(&self, accept: bool) {
        self.accept.set(accept);
    }

    pub fn n_files(&self) -> usize {
        self.files.borrow().len()
    }

    pub fn file(&self, index: usize) -> Option<Item> {
        self.files.borrow().get(index).and_then(|f| f.as_ref()).and_then(|w| w.upgrade())
    }

    pub fn torrent_id(&self) -> i64 {
        self.torrent_id.get()
    }

    pub fn tree_model(&self) -> gtk::TreeListModel {
        self.tree_model
            .borrow_mut()
            .get_or_insert_with(|| {
                gtk::TreeListModel::new(self.store.clone(), false, false, |obj| {
                    obj.downcast_ref::<Item>().and_then(Item::children).map(|c| c.upcast())
                })
            })
            .clone()
    }

    fn item_update(
        &self,
        item: &Item,
        file: &Obj,
        wanted: &[Value],
        priorities: &[Value],
        id: usize,
    ) {
        let length = get_i64(file, p::TFILE_LENGTH);
        let completed = get_i64(file, p::TFILE_BYTES_COMPLETED);
        item.set(COL_SIZE, length);
        item.set(COL_PROGRESS, p::file_get_progress(length, completed));
        item.set(COL_BYTESCOMPLETED, completed);

        if self.accept.get() {
            item.set(COL_WANTED, wanted.get(id).map(prefs::value_as_i64).unwrap_or(0));
            item.set(COL_PRIORITY, priorities.get(id).map(prefs::value_as_i64).unwrap_or(0));

            // the names (of the file and its directories) change on a rename
            let name = get_str(file, p::TFILE_NAME).unwrap_or("");
            let mut path: Vec<&str> = name.split('/').collect();
            if let Some(last) = path.pop() {
                item.set(COL_NAME, last);
                let mut parent = item.parent_item();
                while let (Some(p), Some(n)) = (parent.clone(), path.pop()) {
                    p.set(COL_NAME, n);
                    parent = p.parent_item();
                }
            }
        }
    }

    pub fn update(self: &Rc<Self>, t: &Obj, mode: i32) {
        let files = p::torrent_get_files(t);
        let n_files = files.len();
        let priorities = p::torrent_get_priorities(t);
        let wanted = p::torrent_get_wanted(t);
        let torrent_id = p::torrent_get_id(t);

        if mode == p::TORRENT_GET_MODE_FIRST
            || torrent_id != self.torrent_id.get()
            || self.n_files() != n_files
        {
            // the tree of this torrent is on its way
            if mode != p::TORRENT_GET_MODE_FIRST
                && torrent_id == self.torrent_id.get()
                && self.building.get()
            {
                return;
            }

            self.remove_all();
            self.torrent_id.set(torrent_id);
            let build_id = self.build_id.get();

            if n_files > CREATE_THREAD_IF_GT {
                // build the simple tree in a thread, then add it in the main loop
                self.building.set(true);
                let (files, wanted, priorities) =
                    (files.to_vec(), wanted.to_vec(), priorities.to_vec());
                let model = Rc::downgrade(self);
                glib::spawn_future_local(async move {
                    let top = gio::spawn_blocking(move || {
                        FileNode::from_torrent(&files, &wanted, &priorities)
                    })
                    .await;
                    if let (Ok(top), Some(model)) = (top, model.upgrade()) {
                        // unless another torrent was selected or the model cleared since
                        if build_id == model.build_id.get() {
                            model.set_nodes(&top, false);
                            model.accept.set(true);
                        }
                    }
                });
            } else {
                let top = FileNode::from_torrent(files, wanted, priorities);
                self.set_nodes(&top, false);
                self.accept.set(true);
            }
        } else {
            for (i, file) in files.iter().enumerate() {
                if let (Some(item), Some(file)) = (self.file(i), file.as_object()) {
                    self.item_update(&item, file, wanted, priorities, i);
                }
            }
            for item in item::items(&self.store) {
                update_tree(&item);
            }
        }
    }
}

/// The files view, of a torrent on the server (with a client) or local
pub struct FilesView {
    pub tv: TreeView,
    pub model: Rc<FilesModel>,
    client: Option<Client>,
    win: Option<glib::WeakRef<MainWindow>>,
}

impl FilesView {
    fn create(
        model: &Rc<FilesModel>,
        prefs: Option<&Prefs>,
        config_id: &str,
        progress: bool,
        client: Option<&Client>,
        win: Option<&MainWindow>,
    ) -> Rc<Self> {
        let tv = TreeView::new(prefs, config_id);

        tv.reg_column(ColType::FileIconText, COL_NAME as i32, &tr("Name"), "name", 0)
            .set_extra(COL_ID as i32);
        tv.reg_column(ColType::Size, COL_SIZE as i32, &tr("Size"), "size", 0);
        if progress {
            tv.reg_column(ColType::Prog, COL_PROGRESS as i32, &tr("Progress"), "progress", 0);
        }
        let wanted =
            tv.reg_column(ColType::Wanted, COL_WANTED as i32, &tr("Download"), "wanted", 0);
        let priority =
            tv.reg_column(ColType::Prio, COL_PRIORITY as i32, &tr("Priority"), "priority", 0);

        let view = Rc::new(Self {
            tv: tv.clone(),
            model: model.clone(),
            client: client.cloned(),
            win: win.map(|w| w.downgrade()),
        });

        // A click on the Download check box toggles it
        let weak = Rc::downgrade(&view);
        wanted.set_activate(move |_, item| {
            // a mixed directory is skipped
            set_wanted(item, (item.int64(COL_WANTED) == 0) as i64);
            if let Some(v) = weak.upgrade() {
                v.files_changed();
            }
        });

        // A click on the priority goes from normal to high to low
        let weak = Rc::downgrade(&view);
        priority.set_activate(move |_, item| {
            let next = match item.int64(COL_PRIORITY) {
                TR_PRI_NORMAL => TR_PRI_HIGH,
                TR_PRI_HIGH => TR_PRI_LOW,
                _ => TR_PRI_NORMAL,
            };
            set_priority(item, next);
            if let Some(v) = weak.upgrade() {
                v.files_changed();
            }
        });

        tv.set_search_column(COL_NAME as i32);
        tv.set_model(&model.tree_model());
        tv.setup_columns();
        tv.restore_sort();

        // A new tree is shown expanded
        let weak = tv.downgrade();
        model.connect_rebuilt(move || {
            if let Some(tv) = weak.upgrade() {
                tv.expand_all();
            }
        });

        // Enter or a double click opens or closes a directory
        tv.connect_row_activated(|tv, position| {
            if let Some(row) = tv.selection().item(position).and_downcast::<gtk::TreeListRow>() {
                if row.is_expandable() {
                    row.set_expanded(!row.is_expanded());
                }
            }
        });

        let weak = Rc::downgrade(&view);
        util::add_context_menu(&tv, move |_, x, y| {
            weak.upgrade().is_some_and(|v| v.context_menu(x, y))
        });

        // the tree may be there already
        tv.expand_all();

        view
    }

    pub fn new(
        model: &Rc<FilesModel>,
        win: &MainWindow,
        client: &Client,
        config_id: Option<&str>,
    ) -> Rc<Self> {
        Self::create(
            model,
            Some(client.prefs()),
            config_id.unwrap_or("TrgFilesTreeView"),
            true,
            Some(client),
            Some(win),
        )
    }

    pub fn new_local(model: &Rc<FilesModel>, prefs: Option<&Prefs>, config_id: &str) -> Rc<Self> {
        Self::create(model, prefs, config_id, false, None, None)
    }

    /// Sends a request changing the files, which then get updated. Updates
    /// from the server are ignored meanwhile.
    pub fn dispatch_files_update(self: &Rc<Self>, req: p::Request) {
        let Some(win) = self.win.as_ref().and_then(|w| w.upgrade()) else { return };
        self.model.set_accept(false);
        let view = self.clone();
        win.dispatch_interactive_with(req, move |_| view.model.set_accept(true));
    }

    /// Sends the wanted state and priority of all the files
    fn send_updated_file_prefs(self: &Rc<Self>) {
        if self.client.is_none() {
            return;
        }
        let model = &self.model;
        let mut req = p::torrent_set(&[model.torrent_id()]);

        for i in 0..model.n_files() {
            let Some(item) = model.file(i) else { continue };
            let i = i as i64;
            if item.int64(COL_WANTED) != 0 {
                p::add_file_id_to_array(&mut req, p::FIELD_FILES_WANTED, i);
            } else {
                p::add_file_id_to_array(&mut req, p::FIELD_FILES_UNWANTED, i);
            }
            let key = match item.int64(COL_PRIORITY) {
                TR_PRI_LOW => p::FIELD_FILES_PRIORITY_LOW,
                TR_PRI_HIGH => p::FIELD_FILES_PRIORITY_HIGH,
                _ => p::FIELD_FILES_PRIORITY_NORMAL,
            };
            p::add_file_id_to_array(&mut req, key, i);
        }

        self.dispatch_files_update(req);
    }

    fn files_changed(self: &Rc<Self>) {
        self.tv.resort();
        self.send_updated_file_prefs();
    }

    fn set_selected(self: &Rc<Self>, column: usize, value: i64) {
        for item in self.tv.selected_items() {
            if column == COL_WANTED {
                set_wanted(&item, value);
            } else {
                set_priority(&item, value);
            }
        }
        self.files_changed();
    }

    fn popup_menu(self: &Rc<Self>, x: f64, y: f64) {
        let actions = gio::SimpleActionGroup::new();
        let n_selected = self.tv.count_selected();

        let add = |name: &str, f: Box<dyn Fn(&Rc<Self>)>, enabled: bool| {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(v) = weak.upgrade() {
                    f(&v);
                }
            });
            action.set_enabled(enabled);
            actions.add_action(&action);
        };

        add(
            "rename",
            Box::new(|v| {
                if let (Some(win), Some(client)) =
                    (v.win.as_ref().and_then(|w| w.upgrade()), &v.client)
                {
                    FileRenameDialog::new(&win, client, v).present();
                }
            }),
            // Rename on multiple files currently not supported
            n_selected == 1,
        );
        let sel = n_selected > 0;
        add("priority-high", Box::new(|v| v.set_selected(COL_PRIORITY, TR_PRI_HIGH)), sel);
        add("priority-normal", Box::new(|v| v.set_selected(COL_PRIORITY, TR_PRI_NORMAL)), sel);
        add("priority-low", Box::new(|v| v.set_selected(COL_PRIORITY, TR_PRI_LOW)), sel);
        add("wanted", Box::new(|v| v.set_selected(COL_WANTED, 1)), sel);
        add("unwanted", Box::new(|v| v.set_selected(COL_WANTED, 0)), sel);
        add("expand-all", Box::new(|v| v.tv.expand_all()), true);
        add("collapse-all", Box::new(|v| v.tv.collapse_all()), true);

        let builder = gtk::Builder::from_resource(
            "/io/github/TransmissionRemoteGtk/ui/trg-files-tree-view.ui",
        );
        let menu = gio::Menu::new();

        // Files of a torrent on the server can be renamed
        if self.client.is_some() {
            if let Some(m) = builder.object::<gio::MenuModel>("rename_menu") {
                menu.append_section(None, &m);
            }
        }

        // its sections, as sections of this menu
        if let Some(files_menu) = builder.object::<gio::MenuModel>("files_menu") {
            for i in 0..files_menu.n_items() {
                menu.append_item(&gio::MenuItem::from_model(&files_menu, i));
            }
        }

        util::popover_menu_popup(&self.tv, &menu, x, y, "files", Some(&actions));
    }

    fn context_menu(self: &Rc<Self>, x: f64, y: f64) -> bool {
        // from the keyboard
        if x < 0.0 {
            self.popup_menu(x, y);
            return true;
        }
        // the column headers have their own menu
        if self.tv.point_in_header(x, y) || !self.tv.select_at(x, y) {
            return false;
        }
        self.popup_menu(x, y);
        true
    }
}

/* The rename dialog */

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-file-rename-dialog.ui")]
    pub struct FileRenameDialog {
        #[template_child]
        pub name_entry: TemplateChild<gtk::Entry>,
        pub view: RefCell<Option<Rc<FilesView>>>,
        pub client: RefCell<Option<Client>>,
        pub orig_path: RefCell<Option<String>>,
        pub torrent_id: Cell<i64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileRenameDialog {
        const NAME: &'static str = "TrgFileRenameDialog";
        type Type = super::FileRenameDialog;
        type ParentType = Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for FileRenameDialog {}
    impl WidgetImpl for FileRenameDialog {}
    impl WindowImpl for FileRenameDialog {}
    impl DialogImpl for FileRenameDialog {}
}

glib::wrapper! {
    pub struct FileRenameDialog(ObjectSubclass<imp::FileRenameDialog>)
        @extends Dialog, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root,
            gtk::ShortcutManager;
}

#[gtk::template_callbacks]
impl FileRenameDialog {
    #[template_callback]
    fn name_changed(entry: &gtk::Entry) {
        if let Some(d) = dialog::ancestor::<FileRenameDialog>(entry) {
            d.action_set_enabled("dialog.accept", entry.text_length() > 0);
        }
    }

    pub fn new(win: &MainWindow, client: &Client, view: &Rc<FilesView>) -> Self {
        let d: Self = glib::Object::new();
        let imp = d.imp();
        let tv = &view.tv;
        let item = tv.first_selected();
        let count = tv.count_selected();

        let (orig_filename, orig_path) = match &item {
            Some(item) => (item.string(COL_NAME).unwrap_or_default(), Some(file_item_path(item))),
            None => ("error".to_string(), None),
        };

        imp.name_entry.set_text(&orig_filename);
        *imp.orig_path.borrow_mut() = orig_path;
        imp.torrent_id.set(view.model.torrent_id());
        *imp.view.borrow_mut() = Some(view.clone());
        *imp.client.borrow_mut() = Some(client.clone());

        let msg = if count == 1 {
            sprintf(&tr("Rename %s"), &[&orig_filename])
        } else {
            // this really shouldn't happen since the files view checks the
            // number of selected files before allowing rename
            d.action_set_enabled("dialog.accept", false);
            imp.name_entry.set_sensitive(false);
            sprintf(&tr("INTERNAL ERROR: Rename %d files not supported"), &[&count])
        };

        d.set_transient_for(Some(win));
        d.set_title(Some(&msg));

        d.on_response(|d, response| {
            let imp = d.imp();
            if response == dialog::RESPONSE_ACCEPT {
                let name = imp.name_entry.text();
                let path = imp.orig_path.borrow().clone().unwrap_or_default();
                let req = p::torrent_rename_path(&[imp.torrent_id.get()], &path, &name);
                if let Some(view) = imp.view.borrow().clone() {
                    view.dispatch_files_update(req);
                }
            }
            imp.view.take();
            d.destroy();
        });

        d
    }
}
