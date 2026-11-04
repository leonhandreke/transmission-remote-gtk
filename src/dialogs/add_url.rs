//! Adding a torrent from a URL or magnet link (trg-torrent-add-url-dialog.c)

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, gio, glib};

use crate::client::Client;
use crate::dialog::{self, Dialog, DialogImpl, TrgDialogExt};
use crate::i18n::tr;
use crate::main_window::MainWindow;
use crate::protocol::{self as p, get_bool};
use crate::util;

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-torrent-add-url-dialog.ui")]
    pub struct TorrentAddUrlDialog {
        #[template_child]
        pub url_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub start_check: TemplateChild<gtk::CheckButton>,
        pub client: RefCell<Option<Client>>,
        pub win: glib::WeakRef<MainWindow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TorrentAddUrlDialog {
        const NAME: &'static str = "TrgTorrentAddUrlDialog";
        type Type = super::TorrentAddUrlDialog;
        type ParentType = Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TorrentAddUrlDialog {}
    impl WidgetImpl for TorrentAddUrlDialog {}
    impl WindowImpl for TorrentAddUrlDialog {}
    impl DialogImpl for TorrentAddUrlDialog {}
}

glib::wrapper! {
    pub struct TorrentAddUrlDialog(ObjectSubclass<imp::TorrentAddUrlDialog>)
        @extends Dialog, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root,
            gtk::ShortcutManager;
}

#[gtk::template_callbacks]
impl TorrentAddUrlDialog {
    #[template_callback]
    fn url_entry_changed(entry: &gtk::Entry) {
        if let Some(d) = dialog::ancestor::<TorrentAddUrlDialog>(entry) {
            d.action_set_enabled("dialog.accept", entry.text_length() > 0);
        }
    }

    pub fn new(win: &MainWindow, client: &Client) -> Self {
        let d: Self = glib::Object::new();
        let imp = d.imp();
        *imp.client.borrow_mut() = Some(client.clone());
        imp.win.set(Some(win));

        d.action_set_enabled("dialog.accept", false);

        // a URL or magnet link in the clipboard
        let entry = imp.url_entry.get();
        entry.clipboard().read_text_async(gio::Cancellable::NONE, move |res| {
            if let Ok(Some(text)) = res {
                let is_link =
                    ["http://", "https://", "magnet:"].iter().any(|p| text.starts_with(p));
                if is_link && entry.text().is_empty() {
                    entry.set_text(&text);
                }
            }
        });

        d.set_transient_for(Some(win));
        d.on_response(|d, response| {
            let imp = d.imp();
            if response == dialog::RESPONSE_ACCEPT {
                let client = imp.client.borrow().clone().unwrap();
                let text = imp.url_entry.text().to_string();
                let win = imp.win.upgrade();

                let dht = client.session().map(|s| get_bool(&s, p::SGET_DHT_ENABLED)).unwrap_or(false);
                if text.starts_with("magnet:") && !dht {
                    // The add dialog is destroyed right after this, so use the
                    // main window as parent now that the warning doesn't block.
                    util::error_dialog(
                        win.as_ref(),
                        &tr("You are trying to add a magnet torrent, but DHT is disabled. Distributed Hash Table (DHT) should be enabled in remote settings."),
                    );
                }

                let req = p::torrent_add_url(&text, imp.start_check.is_active());
                let w = imp.win.clone();
                client.dispatch(req, move |r| {
                    if let Some(win) = w.upgrade() {
                        win.on_generic_interactive_action(r);
                    }
                });
            }
            d.destroy();
        });

        d
    }
}
