//! A GtkColumnView (in a scrolled window) of rows, which allows the user to
//! change column visibility with the menu of each column header: uncheck the
//! column to hide it, or choose a hidden column to show it after.
//!
//! The view persists these choices to the preferences, and restores them
//! when it's set up. Column order, widths and the sort column are also saved
//! and restored.
//!
//! All the columns must be registered first, so it knows which property of the
//! row objects they show, what cell widgets etc to use if it should be
//! created, and what columns are available.
//!
//! Sorting is by a registered column, which needn't be a shown column, so the
//! view has its own sorter for the GtkSortListModel and keeps the sort
//! indicator of the column view's headers in sync with it. A column has a
//! numeric key, which is what the sort column is saved as in the preferences:
//! the model column it had when the rows were a column store (like the
//! GtkListStores of the C version), so saved settings keep their meaning.
//!
//! This file has the widget, its model and selection, and showing the rows
//! in a GtkListView instead (set_list_factory). The other parts:

/// The cells' widgets, bound to the row objects
mod cell;
/// Registering, showing and hiding columns, and the header menus
mod column;
/// Editing the cells of editable columns
mod editing;
/// Expanding and collapsing the rows of a tree
mod expand;
/// Saving and restoring the columns and the sorting
mod persist;
/// Finding rows by widget, point and focus, for context menus
mod rows;
/// Type-ahead search
mod search;
/// Sorting, the sort indicator and the sort menu
mod sort;
/// The values of the properties, and their formatting
mod value;

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::{Rc, Weak};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

use crate::prefs::Prefs;
use crate::type_ahead::TypeAhead;
use crate::util;

pub use column::{
    COLUMN_EDITABLE, COLUMN_EXTRA, COLUMN_HIDE_FROM_TOP_MENU, COLUMN_UNREMOVABLE, ColType,
    ColumnDesc,
};
pub use persist::{PERSIST_LAYOUT, PERSIST_SORT};
pub use rows::mark_row_widget;
pub use sort::SortKey;

/// The object of a row of the view's model: the row itself, or the item of a
/// GtkTreeListRow
pub fn row_object(row: glib::Object) -> Option<glib::Object> {
    match row.downcast::<gtk::TreeListRow>() {
        Ok(r) => r.item(),
        Err(row) => Some(row),
    }
}

mod imp {
    use super::*;
    use std::sync::OnceLock;

    pub struct TreeView {
        pub columns: RefCell<Vec<Rc<ColumnDesc>>>,
        pub prefs: RefCell<Option<Prefs>>,
        pub config_id: RefCell<String>,

        pub scrolled: gtk::ScrolledWindow,
        pub column_view: gtk::ColumnView,
        pub list_view: RefCell<Option<gtk::ListView>>,

        pub model: RefCell<Option<gio::ListModel>>,
        pub selection: gtk::MultiSelection,
        /// Sorts the rows (see sort_func)
        pub sorter: OnceCell<gtk::CustomSorter>,

        pub sort_column: Cell<i32>,
        pub sort_order: Cell<gtk::SortType>,
        pub sortable: Cell<bool>,
        pub column_menus: Cell<bool>,
        pub updating_sort: Cell<bool>,

        /// The property of the rows that typing searches
        pub search_column: Cell<Option<&'static str>>,
        pub type_ahead: TypeAhead,

        /// The actions of the column header menus
        pub actions: gio::SimpleActionGroup,
        /// Of the column view's sorter changing, by a click on a header
        pub sorter_handler: RefCell<Option<glib::SignalHandlerId>>,

        /// The cell (a row object and a column) to start editing once it's
        /// shown
        pub start_edit: RefCell<Option<(glib::WeakRef<glib::Object>, Weak<ColumnDesc>)>>,
    }

    impl Default for TreeView {
        fn default() -> Self {
            let selection = gtk::MultiSelection::new(None::<gio::ListModel>);
            Self {
                columns: Default::default(),
                prefs: Default::default(),
                config_id: Default::default(),
                scrolled: gtk::ScrolledWindow::new(),
                column_view: gtk::ColumnView::new(Some(selection.clone())),
                list_view: Default::default(),
                model: Default::default(),
                selection,
                sorter: Default::default(),
                sort_column: Cell::new(-1),
                sort_order: Cell::new(gtk::SortType::Ascending),
                sortable: Cell::new(true),
                column_menus: Cell::new(true),
                updating_sort: Cell::new(false),
                search_column: Cell::new(None),
                type_ahead: Default::default(),
                actions: gio::SimpleActionGroup::new(),
                sorter_handler: Default::default(),
                start_edit: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TreeView {
        const NAME: &'static str = "TrgTreeView";
        type Type = super::TreeView;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for TreeView {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("sort-changed").build(),
                    glib::subclass::Signal::builder("row-activated")
                        .param_types([u32::static_type()])
                        .build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }

        fn dispose(&self) {
            // destroying the column view changes its sorter, which isn't a
            // choice of the user's to sort by
            if let (Some(id), Some(sorter)) =
                (self.sorter_handler.take(), self.column_view.sorter())
            {
                sorter.disconnect(id);
            }
            for desc in self.columns.borrow().iter() {
                desc.column.take();
            }
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for TreeView {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            self.scrolled.measure(orientation, for_size)
        }

        // The context menus are popovers of the view
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.scrolled.allocate(width, height, baseline, None);
            util::present_popovers(&*self.obj());
        }

        fn focus(&self, direction: gtk::DirectionType) -> bool {
            self.scrolled.child_focus(direction)
        }

        fn grab_focus(&self) -> bool {
            self.scrolled.child().is_some_and(|c| c.grab_focus())
        }
    }
}

glib::wrapper! {
    pub struct TreeView(ObjectSubclass<imp::TreeView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TreeView {
    /// A view whose settings are saved in prefs under config_id
    pub fn new(prefs: Option<&Prefs>, config_id: &str) -> Self {
        let tv: Self = glib::Object::new();
        *tv.imp().prefs.borrow_mut() = prefs.cloned();
        *tv.imp().config_id.borrow_mut() = config_id.to_string();
        tv
    }

    fn setup(&self) {
        let imp = self.imp();

        let cv = &imp.column_view;
        cv.set_reorderable(true);
        cv.set_enable_rubberband(true);
        let weak = self.downgrade();
        cv.connect_activate(move |_, position| {
            if let Some(tv) = weak.upgrade() {
                tv.emit_by_name::<()>("row-activated", &[&position]);
            }
        });

        imp.scrolled.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        imp.scrolled.set_has_frame(true);
        imp.scrolled.set_child(Some(cv));
        imp.scrolled.set_parent(self);

        self.setup_sorting();
        self.setup_column_actions();
        self.setup_keys();
    }

    pub fn connect_row_activated<F: Fn(&Self, u32) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("row-activated", false, move |args| {
            f(&args[0].get::<Self>().unwrap(), args[1].get::<u32>().unwrap());
            None
        })
    }

    pub fn connect_sort_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("sort-changed", false, move |args| {
            f(&args[0].get::<Self>().unwrap());
            None
        })
    }

    /// The model is a list model of row objects, or a GtkTreeListModel of
    /// them.
    pub fn set_model(&self, model: &impl IsA<gio::ListModel>) {
        let imp = self.imp();
        let model = model.as_ref().clone();
        let sorter: gtk::Sorter = {
            let s = imp.sorter.get().unwrap().clone();
            if model.is::<gtk::TreeListModel>() {
                gtk::TreeListRowSorter::new(Some(s)).upcast()
            } else {
                s.upcast()
            }
        };
        *imp.model.borrow_mut() = Some(model.clone());
        let sort_model = gtk::SortListModel::new(Some(model), Some(sorter));
        imp.selection.set_model(Some(&sort_model));
    }

    pub fn model(&self) -> Option<gio::ListModel> {
        self.imp().model.borrow().clone()
    }

    pub fn selection(&self) -> gtk::MultiSelection {
        self.imp().selection.clone()
    }

    fn selected_positions(&self) -> Vec<u32> {
        let bitset = self.imp().selection.selection();
        let mut positions = Vec::new();
        if let Some((iter, first)) = gtk::BitsetIter::init_first(&bitset) {
            positions.push(first);
            positions.extend(iter);
        }
        positions
    }

    /// The selected row objects (of type T), in the view's order
    pub fn selected_items<T: IsA<glib::Object>>(&self) -> Vec<T> {
        let sel = &self.imp().selection;
        self.selected_positions()
            .into_iter()
            .filter_map(|p| sel.item(p).and_then(row_object).and_downcast::<T>())
            .collect()
    }

    pub fn count_selected(&self) -> u32 {
        self.imp().selection.selection().size() as u32
    }

    pub fn first_selected<T: IsA<glib::Object>>(&self) -> Option<T> {
        let bitset = self.imp().selection.selection();
        if bitset.is_empty() {
            return None;
        }
        self.imp().selection.item(bitset.minimum()).and_then(row_object).and_downcast::<T>()
    }

    pub fn select_item(&self, item: &impl IsA<glib::Object>) {
        if let Some(position) = self.find_position(item.upcast_ref()) {
            self.select_and_focus(position);
        }
    }

    fn find_position(&self, item: &glib::Object) -> Option<u32> {
        let model = &self.imp().selection;
        (0..model.n_items()).find(|&i| model.item(i).and_then(row_object).as_ref() == Some(item))
    }

    /// Shows the row at position; with flags, also selects it (only it)
    /// and moves the focus there
    fn scroll_to(&self, position: u32, flags: gtk::ListScrollFlags) {
        let imp = self.imp();
        match imp.list_view.borrow().as_ref().filter(|lv| lv.parent().is_some()) {
            Some(lv) => lv.scroll_to(position, flags, None),
            None => {
                imp.column_view.scroll_to(position, None::<&gtk::ColumnViewColumn>, flags, None)
            }
        }
    }

    /// Selects only the row at position, shows it and moves the focus there
    /// (the keyboard focus, if it's in the list)
    fn select_and_focus(&self, position: u32) {
        self.scroll_to(position, gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT);
    }

    /// Shows the rows with factory in a GtkListView instead of the columns,
    /// or the columns again when factory is None.
    pub fn set_list_factory(&self, factory: Option<&gtk::ListItemFactory>) {
        let imp = self.imp();
        match factory {
            Some(factory) => {
                let existing = imp.list_view.borrow().clone();
                let lv = existing.unwrap_or_else(|| {
                    let lv =
                        gtk::ListView::new(None::<gtk::NoSelection>, None::<gtk::ListItemFactory>);
                    let weak = self.downgrade();
                    lv.connect_activate(move |_, position| {
                        if let Some(tv) = weak.upgrade() {
                            tv.emit_by_name::<()>("row-activated", &[&position]);
                        }
                    });
                    *imp.list_view.borrow_mut() = Some(lv.clone());
                    lv
                });
                lv.set_factory(Some(factory));
                lv.set_model(Some(&imp.selection));
                imp.scrolled.set_child(Some(&lv));
            }
            None => {
                if let Some(lv) = imp.list_view.borrow().as_ref() {
                    lv.set_model(None::<&gtk::SelectionModel>);
                    lv.set_factory(None::<&gtk::ListItemFactory>);
                }
                imp.scrolled.set_child(Some(&imp.column_view));
            }
        }
    }
}
