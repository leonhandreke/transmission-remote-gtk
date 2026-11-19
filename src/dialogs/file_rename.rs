//! Renaming a file or directory of a torrent (trg-file-rename-dialog.c)

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::client::Client;
use crate::dialogs::dialog::{Dialog, DialogImpl, Response, TrgDialogExt};
use crate::files::{COL_NAME, FilesView, file_item_path};
use crate::i18n::{sprintf, tr};
use crate::main_window::MainWindow;
use crate::protocol as p;
use crate::util;

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/file-rename-dialog.ui")]
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
        if let Some(d) = util::ancestor::<FileRenameDialog>(entry) {
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
            if response == Response::Accept {
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
