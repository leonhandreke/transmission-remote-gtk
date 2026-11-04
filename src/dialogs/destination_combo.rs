//! An entry for a download directory, with a menu button to choose the
//! default directory, the saved destinations (labels) or the directory of an
//! existing torrent. Emits "changed" when the directory changes
//! (trg-destination-combo.c).

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, gio, glib};

use crate::client::Client;
use crate::dialog;
use crate::prefs;
use crate::protocol::{self as p, get_str};
use crate::util;

#[derive(PartialEq, Eq, Clone, Copy)]
enum DestType {
    Default,
    Label,
    Existing,
}

mod imp {
    use super::*;
    use std::sync::OnceLock;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/trg-destination-combo.ui")]
    pub struct DestinationCombo {
        #[template_child]
        pub entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub button: TemplateChild<gtk::MenuButton>,
        /// label, directory
        pub store: RefCell<Vec<(String, String)>>,
        pub client: RefCell<Option<Client>>,
        pub last_selection: RefCell<Option<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DestinationCombo {
        const NAME: &'static str = "TrgDestinationCombo";
        type Type = super::DestinationCombo;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DestinationCombo {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("changed").build()])
        }
    }

    impl WidgetImpl for DestinationCombo {}
    impl BoxImpl for DestinationCombo {}
}

glib::wrapper! {
    pub struct DestinationCombo(ObjectSubclass<imp::DestinationCombo>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

#[gtk::template_callbacks]
impl DestinationCombo {
    #[template_callback]
    fn trg_destination_combo_entry_changed_cb(entry: &gtk::Entry) {
        if let Some(combo) = dialog::ancestor::<DestinationCombo>(entry) {
            combo.emit_by_name::<()>("changed", &[]);
        }
    }

    pub fn new(client: &Client, last_selection_key: Option<&str>) -> Self {
        let combo: Self = glib::Object::new();
        let imp = combo.imp();
        *imp.client.borrow_mut() = Some(client.clone());
        *imp.last_selection.borrow_mut() = last_selection_key.map(str::to_string);

        combo.load_directory_model();

        let actions = gio::SimpleActionGroup::new();
        let choose = gio::SimpleAction::new("choose", Some(glib::VariantTy::STRING));
        let weak = combo.downgrade();
        choose.connect_activate(move |_, param| {
            if let (Some(c), Some(dir)) =
                (weak.upgrade(), param.and_then(|p| p.str().map(str::to_string)))
            {
                c.imp().entry.set_text(&dir);
            }
        });
        actions.add_action(&choose);
        combo.insert_action_group("destination", Some(&actions));

        // The menu of the button, choosing an item puts its directory in the entry
        let menu = gio::Menu::new();
        for (label, dir) in imp.store.borrow().iter() {
            let item = gio::MenuItem::new(Some(label), None);
            item.set_action_and_target_value(Some("destination.choose"), Some(&dir.to_variant()));
            menu.append_item(&item);
        }
        imp.button.set_menu_model(Some(&menu));

        // Restore any previous selection, or the default dir (the first item)
        let last = last_selection_key
            .and_then(|k| client.prefs().get_string(k, prefs::CONNECTION))
            .filter(|s| !s.is_empty());
        match last {
            Some(dir) => imp.entry.set_text(&dir),
            None => {
                if let Some((_, dir)) = imp.store.borrow().first() {
                    imp.entry.set_text(dir);
                }
            }
        }

        combo
    }

    pub fn connect_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_local("changed", false, move |args| {
            f(&args[0].get::<Self>().unwrap());
            None
        });
    }

    fn insert(&self, label: Option<&str>, dir: &str, t: DestType) {
        let mut store = self.imp().store.borrow_mut();
        if t == DestType::Existing && store.iter().any(|(_, d)| d == dir) {
            return;
        }
        let combo_label = match label {
            Some(l) => format!("{l} ({dir})"),
            None => dir.to_string(),
        };
        store.push((combo_label, dir.to_string()));
    }

    fn load_directory_model(&self) {
        let client = self.imp().client.borrow().clone().unwrap();
        let prefs = client.prefs();

        // the default dir
        let default_dir = client
            .session()
            .and_then(|s| {
                get_str(&s, p::SGET_DOWNLOAD_DIR).map(|d| util::rm_trailing_slashes(d).to_string())
            })
            .unwrap_or_default();
        self.insert(None, &default_dir, DestType::Default);

        // the saved dirs
        if let Some(saved) = prefs.get_array(prefs::KEY_DESTINATIONS, prefs::CONNECTION) {
            for obj in p::objects(&saved) {
                self.insert(
                    get_str(obj, prefs::SUBKEY_LABEL),
                    get_str(obj, prefs::KEY_DESTINATIONS_SUBKEY_DIR).unwrap_or(""),
                    DestType::Label,
                );
            }
        }

        // all previously used download dirs, sorted
        let mut dirs = client.torrent_model().map(|m| m.download_dirs()).unwrap_or_default();
        dirs.sort();
        for dir in dirs.iter().filter(|d| **d != default_dir) {
            self.insert(None, dir, DestType::Existing);
        }
    }

    pub fn dir(&self) -> String {
        self.imp().entry.text().to_string()
    }

    pub fn has_text(&self) -> bool {
        !self.imp().entry.text().is_empty()
    }

    pub fn save_selection(&self) {
        let imp = self.imp();
        if let (Some(key), Some(client)) =
            (imp.last_selection.borrow().as_ref(), imp.client.borrow().as_ref())
        {
            if self.has_text() {
                client.prefs().set_string(key, &self.dir(), prefs::CONNECTION);
            }
        }
    }
}
