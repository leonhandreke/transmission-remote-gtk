//! The files of a torrent as a tree of FileObjects. A directory (ID -1) has its
//! children in a list store; its properties sum up (size, bytes completed,
//! progress) or combine (wanted, priority: TR_PRI_MIXED if its files differ)
//! those of its children.
//!
//! The view is used for the files of the selected torrent, and (locally) for
//! the files of a torrent to add.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

use crate::client::Client;
use crate::i18n::tr;
use crate::main_window::MainWindow;
use crate::prefs::Prefs;
use crate::protocol::{self as p, TR_PRI_HIGH, TR_PRI_LOW, TR_PRI_MIXED, TR_PRI_NORMAL, Torrent};
use crate::torrent_model::UpdateMode;
use crate::tree_view::{ColType, TreeView};
use crate::util::{self, set_changed};

/// Build the tree in a thread for torrents with more files
const CREATE_THREAD_IF_GT: usize = 600;

/* A file or directory of the tree */

mod imp {
    use super::*;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::FileObject)]
    pub struct FileObject {
        /// The name, without the directories
        #[property(get, set)]
        pub name: RefCell<String>,
        /// The index of the file, -1 for a directory
        #[property(get, construct_only)]
        pub id: Cell<i64>,
        #[property(get, set)]
        pub size: Cell<i64>,
        #[property(get, set)]
        pub bytes_completed: Cell<i64>,
        /// In percent
        #[property(get, set)]
        pub progress: Cell<f64>,
        /// 1 or 0, or TR_PRI_MIXED for a directory of both
        #[property(get, set)]
        pub wanted: Cell<i64>,
        /// TR_PRI_*, TR_PRI_MIXED for a directory of several
        #[property(get, set)]
        pub priority: Cell<i64>,
        /// The children of a directory
        pub children: RefCell<Option<gio::ListStore>>,
        /// The directory it's in (not a reference, the parent owns its
        /// children)
        pub parent: glib::WeakRef<super::FileObject>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileObject {
        const NAME: &'static str = "TrgFileObject";
        type Type = super::FileObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for FileObject {}
}

glib::wrapper! {
    pub struct FileObject(ObjectSubclass<imp::FileObject>);
}

impl FileObject {
    pub fn children(&self) -> Option<gio::ListStore> {
        self.imp().children.borrow().clone()
    }

    fn child_objects(&self) -> Vec<FileObject> {
        self.children().map(|c| util::objects(&c).collect()).unwrap_or_default()
    }

    pub fn parent(&self) -> Option<FileObject> {
        self.imp().parent.upgrade()
    }

    /// The path, from the top
    pub fn path(&self) -> String {
        let mut path = self.name();
        let mut parent = self.parent();
        while let Some(p) = parent {
            path = format!("{}/{path}", p.name());
            parent = p.parent();
        }
        path
    }
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
    fn from_torrent(files: &[p::File], wanted: &[bool], priorities: &[i64]) -> Self {
        let mut top = Self::top();
        for (i, file) in files.iter().enumerate() {
            top.insert(
                &file.name,
                file.length,
                file.bytes_completed,
                i as i64,
                wanted.get(i).copied().unwrap_or(false) as i64,
                priorities.get(i).copied().unwrap_or(0),
            );
        }
        top
    }
}

/// Sums up or combines the properties of a directory's children
fn update_dir(dir: &FileObject) {
    if dir.children().is_none() {
        return;
    }
    let (mut size, mut completed) = (0, 0);
    let (mut wanted, mut priority) = (1, TR_PRI_NORMAL);

    for (i, child) in dir.child_objects().iter().enumerate() {
        let (cw, cp) = (child.wanted(), child.priority());
        size += child.size();
        completed += child.bytes_completed();
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

    set_changed(dir, "size", size);
    set_changed(dir, "bytes-completed", completed);
    set_changed(dir, "progress", p::file_progress(size, completed));
    set_changed(dir, "wanted", wanted);
    set_changed(dir, "priority", priority);
}

/// Updates the directories of a subtree, the deepest first
fn update_tree(file: &FileObject) {
    if file.children().is_none() {
        return;
    }
    for child in file.child_objects() {
        update_tree(&child);
    }
    update_dir(file);
}

/// Sets a property of a file, or of all of a directory, and updates the
/// directories above
fn set_property(file: &FileObject, name: &str, value: i64) {
    fn set_subtree(file: &FileObject, name: &str, value: i64) {
        for child in file.child_objects() {
            set_subtree(&child, name, value);
        }
        set_changed(file, name, value);
    }
    set_subtree(file, name, value);
    let mut parent = file.parent();
    while let Some(p) = parent {
        update_dir(&p);
        parent = p.parent();
    }
}

/// What sets a property of files, set_wanted or set_priority
pub type FileSetter = fn(&FileObject, i64);

pub fn set_wanted(file: &FileObject, wanted: i64) {
    set_property(file, "wanted", wanted);
}

pub fn set_priority(file: &FileObject, priority: i64) {
    set_property(file, "priority", priority);
}

pub struct FilesModel {
    pub store: gio::ListStore,
    torrent_id: Cell<i64>,
    accept: Cell<bool>,
    /// the files by their index
    files: RefCell<Vec<Option<glib::WeakRef<FileObject>>>>,
    tree_model: RefCell<Option<gtk::TreeListModel>>,
    /// a tree being built in a thread
    build_id: Cell<u32>,
    building: Cell<bool>,
    rebuilt: RefCell<Vec<Box<dyn Fn()>>>,
}

impl FilesModel {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            store: gio::ListStore::new::<FileObject>(),
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

    fn file_new(&self, parent: Option<&FileObject>, node: &FileNode, defaults: bool) -> FileObject {
        let is_dir = !node.children.is_empty();
        let mut builder = glib::Object::builder::<FileObject>()
            .property("name", node.name.clone().unwrap_or_default())
            .property("id", if is_dir { -1 } else { node.index });
        if !is_dir {
            builder = builder.property("size", node.length);
            builder = if defaults {
                builder.property("wanted", 1i64).property("priority", TR_PRI_NORMAL)
            } else {
                builder
                    .property("bytes-completed", node.bytes_completed)
                    .property("progress", p::file_progress(node.length, node.bytes_completed))
                    .property("wanted", node.enabled)
                    .property("priority", node.priority)
            };
        }
        let file = builder.build();
        file.imp().parent.set(parent);

        if is_dir {
            let children: Vec<FileObject> =
                node.children.iter().map(|c| self.file_new(Some(&file), c, defaults)).collect();
            let store = gio::ListStore::new::<FileObject>();
            store.splice(0, 0, &children);
            *file.imp().children.borrow_mut() = Some(store);
        } else if node.index >= 0 {
            let mut files = self.files.borrow_mut();
            let index = node.index as usize;
            if index >= files.len() {
                files.resize(index + 1, None);
            }
            files[index] = Some(file.downgrade());
        }
        file
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
        let files: Vec<FileObject> = if top.name.is_some() {
            vec![self.file_new(None, top, defaults)]
        } else {
            top.children.iter().map(|c| self.file_new(None, c, defaults)).collect()
        };
        for file in &files {
            update_tree(file);
        }
        // the top level in one go
        self.store.splice(0, 0, &files);
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

    pub fn file(&self, index: usize) -> Option<FileObject> {
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
                    obj.downcast_ref::<FileObject>()
                        .and_then(FileObject::children)
                        .map(|c| c.upcast())
                })
            })
            .clone()
    }

    fn file_update(
        &self,
        obj: &FileObject,
        file: &p::File,
        wanted: &[bool],
        priorities: &[i64],
        id: usize,
    ) {
        let (length, completed) = (file.length, file.bytes_completed);
        set_changed(obj, "size", length);
        set_changed(obj, "progress", p::file_progress(length, completed));
        set_changed(obj, "bytes-completed", completed);

        if self.accept.get() {
            set_changed(obj, "wanted", wanted.get(id).copied().unwrap_or(false) as i64);
            set_changed(obj, "priority", priorities.get(id).copied().unwrap_or(0));

            // the names (of the file and its directories) change on a rename
            let mut path: Vec<&str> = file.name.split('/').collect();
            if let Some(last) = path.pop() {
                set_changed(obj, "name", last.to_string());
                let mut parent = obj.parent();
                while let (Some(p), Some(n)) = (parent.clone(), path.pop()) {
                    set_changed(&p, "name", n.to_string());
                    parent = p.parent();
                }
            }
        }
    }

    pub fn update(self: &Rc<Self>, t: &Torrent, mode: UpdateMode) {
        let (files, wanted, priorities) = (&t.files, &t.wanted, &t.priorities);
        let n_files = files.len();
        let torrent_id = t.id;

        if mode == UpdateMode::First
            || torrent_id != self.torrent_id.get()
            || self.n_files() != n_files
        {
            // the tree of this torrent is on its way
            if mode != UpdateMode::First
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
                    (files.clone(), wanted.clone(), priorities.clone());
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
                if let Some(obj) = self.file(i) {
                    self.file_update(&obj, file, wanted, priorities, i);
                }
            }
            for obj in util::objects(&self.store) {
                update_tree(&obj);
            }
        }
    }
}

/// The files view, of a torrent on the server (with a client) or local
/// An action of the files view's context menu
type FilesAction = Box<dyn Fn(&Rc<FilesView>)>;

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

        // the keys are the model columns the files had as Items
        let reg = |t, key, p, h: &str, id: &str| tv.reg_property(t, key, p, h, id, 0);
        reg(ColType::FileIconText, 0, "name", &tr("Name"), "name").set_extra("id");
        reg(ColType::Size, 1, "size", &tr("Size"), "size");
        if progress {
            reg(ColType::Prog, 2, "progress", &tr("Progress"), "progress");
        }
        let wanted = reg(ColType::Wanted, 4, "wanted", &tr("Download"), "wanted");
        let priority = reg(ColType::Prio, 5, "priority", &tr("Priority"), "priority");

        let view = Rc::new(Self {
            tv: tv.clone(),
            model: model.clone(),
            client: client.cloned(),
            win: win.map(|w| w.downgrade()),
        });

        // A click on the Download check box toggles it
        let weak = Rc::downgrade(&view);
        wanted.set_activate(move |_, file: &FileObject| {
            // a mixed directory is skipped
            set_wanted(file, (file.wanted() == 0) as i64);
            if let Some(v) = weak.upgrade() {
                v.files_changed();
            }
        });

        // A click on the priority goes from normal to high to low
        let weak = Rc::downgrade(&view);
        priority.set_activate(move |_, file: &FileObject| {
            let next = match file.priority() {
                TR_PRI_NORMAL => TR_PRI_HIGH,
                TR_PRI_HIGH => TR_PRI_LOW,
                _ => TR_PRI_NORMAL,
            };
            set_priority(file, next);
            if let Some(v) = weak.upgrade() {
                v.files_changed();
            }
        });

        tv.set_search_column("name");
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
            if let Some(row) = tv.selection().item(position).and_downcast::<gtk::TreeListRow>()
                && row.is_expandable()
            {
                row.set_expanded(!row.is_expanded());
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
        let Some(client) = &self.client else { return };
        self.model.set_accept(false);
        let view = self.clone();
        client.dispatch_interactive_with(req, move |_| view.model.set_accept(true));
    }

    /// Sends the wanted state and priority of all the files
    fn send_updated_file_prefs(self: &Rc<Self>) {
        if self.client.is_none() {
            return;
        }
        let model = &self.model;
        let mut req = p::torrent_set(&[model.torrent_id()]);

        for i in 0..model.n_files() {
            let Some(file) = model.file(i) else { continue };
            let i = i as i64;
            if file.wanted() != 0 {
                p::add_file_id_to_array(&mut req, p::FIELD_FILES_WANTED, i);
            } else {
                p::add_file_id_to_array(&mut req, p::FIELD_FILES_UNWANTED, i);
            }
            let key = match file.priority() {
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

    /// Sets the wanted state or priority (with set_wanted or set_priority)
    /// of the selected files
    fn set_selected(self: &Rc<Self>, set: FileSetter, value: i64) {
        for file in self.tv.selected_items::<FileObject>() {
            set(&file, value);
        }
        self.files_changed();
    }

    fn popup_menu(self: &Rc<Self>, x: f64, y: f64) {
        let actions = gio::SimpleActionGroup::new();
        let n_selected = self.tv.count_selected();

        let add = |name: &str, f: FilesAction, enabled: bool| {
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
                    crate::dialogs::file_rename::FileRenameDialog::new(&win, client, v).present();
                }
            }),
            // Rename on multiple files currently not supported
            n_selected == 1,
        );
        let sel = n_selected > 0;
        add("priority-high", Box::new(|v| v.set_selected(set_priority, TR_PRI_HIGH)), sel);
        add("priority-normal", Box::new(|v| v.set_selected(set_priority, TR_PRI_NORMAL)), sel);
        add("priority-low", Box::new(|v| v.set_selected(set_priority, TR_PRI_LOW)), sel);
        add("wanted", Box::new(|v| v.set_selected(set_wanted, 1)), sel);
        add("unwanted", Box::new(|v| v.set_selected(set_wanted, 0)), sel);
        add("expand-all", Box::new(|v| v.tv.expand_all()), true);
        add("collapse-all", Box::new(|v| v.tv.collapse_all()), true);

        let builder =
            gtk::Builder::from_resource("/io/github/TransmissionRemoteGtk/ui/files-tree-view.ui");
        let menu = gio::Menu::new();

        // Files of a torrent on the server can be renamed
        if self.client.is_some()
            && let Some(m) = builder.object::<gio::MenuModel>("rename_menu")
        {
            menu.append_section(None, &m);
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
        let for_rows = self.tv.context_menu_for_rows(x, y);
        if for_rows {
            self.popup_menu(x, y);
        }
        for_rows
    }
}
