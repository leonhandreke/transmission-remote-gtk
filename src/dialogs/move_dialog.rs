//! Moving torrents to another location (trg-torrent-move-dialog.c)

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::client::Client;
use crate::dialog::{self, Dialog, DialogImpl, TrgDialogExt};
use crate::dialogs::destination_combo::DestinationCombo;
use crate::i18n::{sprintf, tr};
use crate::main_window::MainWindow;
use crate::prefs;
use crate::protocol as p;
use crate::torrent_view;
use crate::tree_view::TreeView;

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-torrent-move-dialog.ui")]
    pub struct TorrentMoveDialog {
        #[template_child]
        pub grid: TemplateChild<gtk::Grid>,
        #[template_child]
        pub location_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub move_check: TemplateChild<gtk::CheckButton>,
        pub location_combo: RefCell<Option<DestinationCombo>>,
        pub ids: RefCell<Vec<i64>>,
        pub client: RefCell<Option<Client>>,
        pub win: glib::WeakRef<MainWindow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TorrentMoveDialog {
        const NAME: &'static str = "TrgTorrentMoveDialog";
        type Type = super::TorrentMoveDialog;
        type ParentType = Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TorrentMoveDialog {}
    impl WidgetImpl for TorrentMoveDialog {}
    impl WindowImpl for TorrentMoveDialog {}
    impl DialogImpl for TorrentMoveDialog {}
}

glib::wrapper! {
    pub struct TorrentMoveDialog(ObjectSubclass<imp::TorrentMoveDialog>)
        @extends Dialog, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root,
            gtk::ShortcutManager;
}

impl TorrentMoveDialog {
    pub fn new(win: &MainWindow, client: &Client, tv: &TreeView) -> Self {
        let d: Self = glib::Object::new();
        let imp = d.imp();
        *imp.client.borrow_mut() = Some(client.clone());
        imp.win.set(Some(win));

        let combo = DestinationCombo::new(client, Some(prefs::KEY_LAST_MOVE_DESTINATION));
        combo.set_hexpand(true);
        let weak = d.downgrade();
        combo.connect_changed(move |combo| {
            if let Some(d) = weak.upgrade() {
                d.action_set_enabled("dialog.accept", combo.has_text());
            }
        });
        imp.grid.attach(&combo, 1, 0, 1, 1);
        imp.location_label.set_mnemonic_widget(Some(&combo));
        d.action_set_enabled("dialog.accept", combo.has_text());
        *imp.location_combo.borrow_mut() = Some(combo);

        let count = tv.count_selected();
        *imp.ids.borrow_mut() = torrent_view::selected_ids(tv);

        let msg = if count == 1 {
            let name = client
                .torrent_data(win.selected_torrent_id())
                .map(|(t, _)| p::torrent_get_name(&t).to_string())
                .unwrap_or_default();
            sprintf(&tr("Move %s"), &[&name])
        } else {
            sprintf(&tr("Move %d torrents"), &[&count])
        };

        d.set_transient_for(Some(win));
        d.set_title(Some(&msg));

        d.on_response(|d, response| {
            let imp = d.imp();
            if response == dialog::RESPONSE_ACCEPT {
                let combo = imp.location_combo.borrow().clone().unwrap();
                let req = p::torrent_set_location(
                    &imp.ids.borrow(),
                    &combo.dir(),
                    imp.move_check.is_active(),
                );
                combo.save_selection();
                let w = imp.win.clone();
                imp.client.borrow().as_ref().unwrap().dispatch(req, move |r| {
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
