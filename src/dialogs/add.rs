//! Adding torrents from files, with options: the destination, the priority
//! and the files to download (trg-torrent-add-dialog.c), or straight away.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, gio, glib};

use crate::bencode;
use crate::client::Client;
use crate::dialog::{self, Dialog, DialogImpl, TrgDialogExt};
use crate::dialogs::destination_combo::DestinationCombo;
use crate::files::{self, FilesModel, FilesView};
use crate::i18n::{sprintf, tr};
use crate::item;
use crate::main_window::MainWindow;
use crate::prefs::{self, Prefs};
use crate::protocol::{self as p, TR_PRI_HIGH, TR_PRI_LOW, TR_PRI_NORMAL};
use crate::upload::{self, Upload};
use crate::util;

const MAGNET_MAX_LINK_WIDTH: usize = 75;

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-torrent-add-dialog.ui")]
    pub struct TorrentAddDialog {
        #[template_child]
        pub source_chooser: TemplateChild<gtk::Button>,
        #[template_child]
        pub priority_combo: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub paused_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub delete_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub sequential_check: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub dest_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub dest_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub file_list_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub apply_all_button: TemplateChild<gtk::MenuButton>,

        pub client: RefCell<Option<Client>>,
        pub win: glib::WeakRef<MainWindow>,
        pub filenames: RefCell<Vec<String>>,
        pub dest_combo: RefCell<Option<DestinationCombo>>,
        pub files_model: RefCell<Option<Rc<FilesModel>>>,
        pub file_list: RefCell<Option<Rc<FilesView>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TorrentAddDialog {
        const NAME: &'static str = "TrgTorrentAddDialog";
        type Type = super::TorrentAddDialog;
        type ParentType = Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TorrentAddDialog {}
    impl WidgetImpl for TorrentAddDialog {}
    impl WindowImpl for TorrentAddDialog {}
    impl DialogImpl for TorrentAddDialog {}
}

glib::wrapper! {
    pub struct TorrentAddDialog(ObjectSubclass<imp::TorrentAddDialog>)
        @extends Dialog, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root,
            gtk::ShortcutManager;
}

/// The file chooser for adding torrents
fn generic_dialog(prefs: &Prefs) -> gtk::FileDialog {
    let dialog = gtk::FileDialog::builder()
        .title(tr("Add a Torrent"))
        .accept_label(tr("_Add"))
        .modal(true)
        .build();

    if let Some(dir) = prefs.get_string(prefs::KEY_LAST_TORRENT_DIR, prefs::GLOBAL) {
        dialog.set_initial_folder(Some(&gio::File::for_path(dir)));
    }

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    let torrents = gtk::FileFilter::new();
    torrents.set_name(Some(&tr("Torrent files")));
    torrents.add_pattern("*.torrent");
    filters.append(&torrents);
    dialog.set_default_filter(Some(&torrents));
    let all = gtk::FileFilter::new();
    all.set_name(Some(&tr("All files")));
    all.add_pattern("*");
    filters.append(&all);
    dialog.set_filters(Some(&filters));

    dialog
}

fn filenames(files: &gio::ListModel) -> Vec<String> {
    (0..files.n_items())
        .filter_map(|i| files.item(i).and_downcast::<gio::File>())
        .filter_map(|f| f.path().map(|p| p.to_string_lossy().into_owned()))
        .collect()
}

/// Remember the folder of the chosen files for the next time
fn save_dir(files: &gio::ListModel, prefs: &Prefs) {
    let dir =
        files.item(0).and_downcast::<gio::File>().and_then(|f| f.parent()).and_then(|p| p.path());
    if let Some(dir) = dir {
        prefs.set_string(prefs::KEY_LAST_TORRENT_DIR, &dir.to_string_lossy(), prefs::GLOBAL);
    }
}

/// Add (File -> Add): choose files, then add them, with the options dialog
/// if that's what the preferences say
pub fn add_dialog(win: &MainWindow, client: &Client) {
    let dialog = generic_dialog(client.prefs());
    let w = win.downgrade();
    let client = client.clone();
    dialog.open_multiple(Some(win), gio::Cancellable::NONE, move |result| {
        let (Ok(files), Some(win)) = (result, w.upgrade()) else { return };
        if files.n_items() == 0 {
            return;
        }
        let prefs = client.prefs();
        // A GtkFileDialog can't have extra options, so whether the options
        // dialog is shown follows the preference ("Options dialog on add").
        let show_options = prefs.get_bool(prefs::KEY_ADD_OPTIONS_DIALOG, prefs::GLOBAL);
        let list = filenames(&files);
        save_dir(&files, prefs);

        if show_options {
            TorrentAddDialog::new_from_filenames(&win, &client, list).present();
        } else {
            upload::do_upload(Upload::new(list, &client, &win, prefs.add_flags()));
        }
    });
}

/// The choices of "Apply to all", the targets of the items of its menu
const APPLY_ALL_CHOICES: &[(&str, usize, i64)] = &[
    ("high", files::COL_PRIORITY, TR_PRI_HIGH),
    ("normal", files::COL_PRIORITY, TR_PRI_NORMAL),
    ("low", files::COL_PRIORITY, TR_PRI_LOW),
    ("download", files::COL_WANTED, 1),
    ("skip", files::COL_WANTED, 0),
];

#[gtk::template_callbacks]
impl TorrentAddDialog {
    #[template_callback]
    fn source_click_cb(button: &gtk::Button) {
        let Some(d) = dialog::ancestor::<TorrentAddDialog>(button) else { return };
        let dialog = generic_dialog(d.client().prefs());
        let weak = d.downgrade();
        dialog.open_multiple(Some(&d), gio::Cancellable::NONE, move |result| {
            let (Ok(files), Some(d)) = (result, weak.upgrade()) else { return };
            if files.n_items() > 0 {
                save_dir(&files, d.client().prefs());
                d.set_filenames(filenames(&files));
            }
        });
    }

    pub fn new_from_filenames(win: &MainWindow, client: &Client, filenames: Vec<String>) -> Self {
        let d: Self = glib::Object::new();
        let imp = d.imp();
        *imp.client.borrow_mut() = Some(client.clone());
        imp.win.set(Some(win));
        let prefs = client.prefs();

        d.set_transient_for(Some(win));

        let files_model = FilesModel::new();
        let file_list = FilesView::new_local(&files_model, None, "TrgFilesTreeView");
        file_list.tv.set_sensitive(false);
        file_list.tv.set_size_request(466, 300);
        file_list.tv.set_vexpand(true);
        imp.file_list_box.append(&file_list.tv);

        let actions = gio::SimpleActionGroup::new();
        let apply_all = gio::SimpleAction::new("apply-all", Some(glib::VariantTy::STRING));
        let weak = d.downgrade();
        apply_all.connect_activate(move |_, param| {
            if let (Some(d), Some(name)) =
                (weak.upgrade(), param.and_then(|p| p.str().map(str::to_string)))
            {
                d.apply_all(&name);
            }
        });
        actions.add_action(&apply_all);
        d.insert_action_group("add", Some(&actions));

        // nothing to apply to without a file list
        file_list
            .tv
            .bind_property("sensitive", &imp.apply_all_button.get(), "sensitive")
            .sync_create()
            .build();

        imp.paused_check.set_active(prefs.get_bool(prefs::KEY_START_PAUSED, prefs::GLOBAL));
        imp.delete_check.set_active(prefs.get_bool(prefs::KEY_DELETE_LOCAL_TORRENT, prefs::GLOBAL));

        // the server's default for added torrents
        match client.session().and_then(|s| s.get(p::SGET_SEQUENTIAL_DOWNLOAD).cloned()) {
            Some(v) => imp.sequential_check.set_active(v.as_bool().unwrap_or(false)),
            None => imp.sequential_check.set_visible(false),
        }

        // the selected position - 1 is the priority (TR_PRI_LOW is -1)
        imp.priority_combo.set_selected(1);

        *imp.files_model.borrow_mut() = Some(files_model);
        *imp.file_list.borrow_mut() = Some(file_list);

        if !filenames.is_empty() {
            d.set_filenames(filenames);
        }

        let dest = DestinationCombo::new(client, Some(prefs::KEY_LAST_ADD_DESTINATION));
        dest.set_hexpand(true);
        imp.dest_box.append(&dest);
        imp.dest_label.set_mnemonic_widget(Some(&dest));
        *imp.dest_combo.borrow_mut() = Some(dest);

        d.on_response(|d, response| d.response_cb(response));

        d
    }

    fn client(&self) -> Client {
        self.imp().client.borrow().clone().unwrap()
    }

    fn files_model(&self) -> Rc<FilesModel> {
        self.imp().files_model.borrow().clone().unwrap()
    }

    fn file_list(&self) -> Rc<FilesView> {
        self.imp().file_list.borrow().clone().unwrap()
    }

    fn apply_all(&self, name: &str) {
        let Some(&(_, column, value)) = APPLY_ALL_CHOICES.iter().find(|c| c.0 == name) else {
            return;
        };
        for item in item::items(&self.files_model().store) {
            if column == files::COL_WANTED {
                files::set_wanted(&item, value);
            } else {
                files::set_priority(&item, value);
            }
        }
        self.file_list().tv.resort();
    }

    fn set_filenames(&self, filenames: Vec<String>) {
        let imp = self.imp();
        let chooser = &imp.source_chooser;
        let files_model = self.files_model();
        let file_list = self.file_list();
        let win = imp.win.upgrade();

        files_model.clear();

        if filenames.len() == 1 {
            let file_name = &filenames[0];
            if util::is_url(file_name) || util::is_magnet(file_name) {
                if file_name.chars().count() > MAGNET_MAX_LINK_WIDTH {
                    let trunc: String = file_name.chars().take(MAGNET_MAX_LINK_WIDTH).collect();
                    chooser.set_label(&format!("{trunc} ..."));
                } else {
                    chooser.set_label(file_name);
                }
                file_list.tv.set_sensitive(false);
                imp.delete_check.set_sensitive(false);
            } else {
                let base = std::path::Path::new(file_name)
                    .file_name()
                    .map(|b| b.to_string_lossy().into_owned())
                    .unwrap_or_else(|| file_name.clone());
                chooser.set_label(&base);

                let mut parsed = false;
                if std::path::Path::new(file_name).is_file() {
                    match bencode::parse_torrent_file(file_name) {
                        Err(e) => {
                            util::error_dialog(win.as_ref(), &e);
                            imp.filenames.borrow_mut().clear();
                            return;
                        }
                        Ok(Some(t)) => {
                            files_model.set_tree(&t.top_node);
                            parsed = true;
                        }
                        Ok(None) => {}
                    }
                } else {
                    util::error_dialog(
                        win.as_ref(),
                        &sprintf(&tr("Unable to open torrent file: %s"), &[file_name]),
                    );
                }
                file_list.tv.set_sensitive(parsed);
            }
        } else {
            file_list.tv.set_sensitive(false);
            chooser.set_label(&if filenames.is_empty() { tr("(None)") } else { tr("(Multiple)") });
        }

        *imp.filenames.borrow_mut() = filenames;
    }

    fn response_cb(&self, response: i32) {
        let imp = self.imp();
        let mut flags = 0;
        if imp.paused_check.is_active() {
            flags |= p::TORRENT_ADD_FLAG_PAUSED;
        }
        if imp.delete_check.is_active() {
            flags |= p::TORRENT_ADD_FLAG_DELETE;
        }

        if response == dialog::RESPONSE_ACCEPT
            && let Some(win) = imp.win.upgrade()
        {
            let dest = imp.dest_combo.borrow().clone().unwrap();
            let model = self.files_model();
            let mut up = Upload::new(imp.filenames.take(), &self.client(), &win, flags);
            up.dir = Some(dest.dir());
            up.priority = imp.priority_combo.selected() as i64 - 1;
            up.extra_args = true;
            if imp.sequential_check.is_visible() {
                up.sequential_download = Some(imp.sequential_check.is_active());
            }

            // The wanted state and priority of each file of the torrent
            let n = model.n_files();
            let mut wanted = vec![false; n];
            let mut priorities = vec![0; n];
            for i in 0..n {
                if let Some(item) = model.file(i) {
                    wanted[i] = item.int64(files::COL_WANTED) != 0;
                    priorities[i] = item.int64(files::COL_PRIORITY);
                }
            }
            up.file_wanted = Some(wanted);
            up.file_priorities = Some(priorities);

            upload::do_upload(up);
            dest.save_selection();
        }

        self.destroy();
    }
}
