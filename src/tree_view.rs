//! A GtkColumnView (in a scrolled window) of Items, which allows the user to
//! change column visibility with the menu of each column header: uncheck the
//! column to hide it, or choose a hidden column to show it after.
//!
//! The view persists these choices to the preferences, and restores them
//! when it's set up. Column order, widths and the sort column are also saved
//! and restored.
//!
//! All the columns must be registered first, so it knows what model column,
//! cell widgets etc to use if it should be created, and what columns are
//! available.
//!
//! Sorting is by a model column, which needn't be a shown column, so the view
//! has its own sorter for the GtkSortListModel and keeps the sort indicator of
//! the column view's headers in sync with it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib, graphene};
use serde_json::{Value, json};

use crate::i18n::tr;
use crate::item::{self, Item, Val};
use crate::prefs::{self, Prefs};
use crate::protocol::{TR_PRI_HIGH, TR_PRI_LOW, TR_PRI_MIXED, TR_PRI_NORMAL};
use crate::util;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColType {
    IconText,
    FileIconText,
    Wanted,
    Text,
    Size,
    Ratio,
    Epoch,
    Speed,
    Eta,
    Prog,
    Prio,
    NumGtZero,
    NumGtEqZero,
}

pub const COLUMN_SHOWING: u32 = 1 << 0;
pub const COLUMN_UNREMOVABLE: u32 = 1 << 1;
pub const COLUMN_EXTRA: u32 = 1 << 2;
pub const COLUMN_HIDE_FROM_TOP_MENU: u32 = 1 << 3;
pub const COLUMN_EDITABLE: u32 = 1 << 4;

pub const PERSIST_SORT: u32 = 1 << 0;
pub const PERSIST_LAYOUT: u32 = 1 << 1;

pub const SORT_TYPE_ACTION: &str = "sort-type";
pub const SORT_COLUMN_ACTION: &str = "sort-column";

const DATA_CELL: &str = "trg-cell";
const DATA_LIST_ITEM: &str = "trg-list-item";

/// Called when the user toggles a "wanted" cell or clicks a priority cell
pub type ActivateFn = Rc<dyn Fn(&TreeView, &Item)>;
/// Called when the user starts editing an editable text cell (done is false,
/// new_text None), and when they finish (done is true, new_text is None if
/// the text wasn't changed).
pub type EditFn = Rc<dyn Fn(&TreeView, &Item, bool, Option<&str>)>;

pub struct ColumnDesc {
    pub model_column: i32,
    pub model_column_extra: Cell<i32>,
    pub header: String,
    pub id: String,
    pub flags: Cell<u32>,
    pub ctype: ColType,
    pub activate: RefCell<Option<ActivateFn>>,
    pub edit: RefCell<Option<EditFn>>,
    pub editable: Cell<bool>,
    column: RefCell<Option<gtk::ColumnViewColumn>>,
}

impl ColumnDesc {
    pub fn set_extra(&self, column: i32) {
        self.model_column_extra.set(column);
    }

    pub fn set_activate(&self, f: impl Fn(&TreeView, &Item) + 'static) {
        *self.activate.borrow_mut() = Some(Rc::new(f));
    }

    pub fn set_edit(&self, f: impl Fn(&TreeView, &Item, bool, Option<&str>) + 'static) {
        *self.edit.borrow_mut() = Some(Rc::new(f));
    }

    pub fn set_editable(&self, editable: bool) {
        self.editable.set(editable);
    }

    fn showing(&self) -> bool {
        self.flags.get() & COLUMN_SHOWING != 0
    }
}

/// A cell (the widgets of a column of a row) and what it's bound to
struct CellState {
    desc: Rc<ColumnDesc>,
    tv: glib::WeakRef<TreeView>,
    list_item: glib::WeakRef<gtk::ListItem>,
    expander: Option<gtk::TreeExpander>,
    image: Option<gtk::Image>,
    widget: gtk::Widget,
    item: RefCell<Option<Item>>,
    handlers: RefCell<Vec<glib::SignalHandlerId>>,
    updating: Cell<bool>,
    edit_original: RefCell<Option<String>>,
}

fn cell_of(obj: &impl IsA<glib::Object>) -> Option<Rc<CellState>> {
    unsafe { obj.data::<Rc<CellState>>(DATA_CELL).map(|p| p.as_ref().clone()) }
}

/// For row widgets of a list factory (and cells), so their rows can be found
pub fn mark_row_widget(widget: &impl IsA<gtk::Widget>, list_item: &gtk::ListItem) {
    unsafe {
        widget.set_data(DATA_LIST_ITEM, list_item.downgrade());
    }
}

fn marked_list_item(widget: &gtk::Widget) -> Option<gtk::ListItem> {
    unsafe {
        widget
            .data::<glib::WeakRef<gtk::ListItem>>(DATA_LIST_ITEM)
            .and_then(|p| p.as_ref().upgrade())
    }
}

/// The icon of a file (by its name), or of a directory (file id -1)
fn file_icon_for(file_id: i64, name: Option<&str>) -> gio::Icon {
    if file_id == -1 {
        return gio::ThemedIcon::new("folder").upcast();
    }
    #[cfg(not(windows))]
    if let Some(name) = name {
        let (mime, uncertain) = gio::content_type_guess(Some(name), None::<&[u8]>);
        if !uncertain {
            return gio::content_type_get_icon(&mime);
        }
    }
    gio::ThemedIcon::new("text-x-generic").upcast()
}

/// Value formatting, like the cell renderers of the GtkTreeView did
fn format_value(desc: &ColumnDesc, value: &Val) -> Option<String> {
    match desc.ctype {
        ColType::Speed => {
            let speed = value.as_i64();
            (speed > 0).then(|| util::format_speed(speed))
        }
        ColType::Epoch => {
            let epoch = value.as_i64();
            (epoch > 0).then(|| util::epoch_to_string(epoch))
        }
        ColType::Eta => {
            let eta = value.as_i64();
            if eta > 0 {
                Some(util::format_time_short(eta))
            } else if eta == -2 {
                Some("\u{221E}".into())
            } else {
                None
            }
        }
        ColType::Size => {
            let size = value.as_i64();
            (size > 0).then(|| util::format_size(size))
        }
        ColType::Ratio => {
            let ratio = value.as_f64();
            (ratio > 0.0).then(|| util::format_ratio(ratio))
        }
        ColType::Prio => match value.as_i64() {
            TR_PRI_LOW => Some(tr("Low")),
            TR_PRI_HIGH => Some(tr("High")),
            TR_PRI_NORMAL => Some(tr("Normal")),
            TR_PRI_MIXED => Some(tr("Mixed")),
            _ => None,
        },
        ColType::NumGtZero | ColType::NumGtEqZero => {
            let v = value.as_i64();
            let min = if desc.ctype == ColType::NumGtZero { 1 } else { 0 };
            (v >= min).then(|| v.to_string())
        }
        _ => value.to_text(),
    }
}

impl CellState {
    fn refresh(&self) {
        let Some(item) = self.item.borrow().clone() else {
            return;
        };
        let desc = &self.desc;
        let value = item.get(desc.model_column as usize);
        self.updating.set(true);

        match desc.ctype {
            ColType::Prog => {
                let progress = value.as_f64().clamp(0.0, 100.0);
                let bar = self.widget.downcast_ref::<gtk::ProgressBar>().unwrap();
                bar.set_fraction(progress / 100.0);
                bar.set_text(Some(&util::progress_text(progress)));
            }
            ColType::Wanted => {
                let wanted = value.as_i64();
                let check = self.widget.downcast_ref::<gtk::CheckButton>().unwrap();
                check.set_inconsistent(wanted == TR_PRI_MIXED);
                check.set_active(wanted == 1);
            }
            ColType::IconText | ColType::FileIconText => {
                let text = value.to_text();
                self.widget
                    .downcast_ref::<gtk::Label>()
                    .unwrap()
                    .set_text(text.as_deref().unwrap_or(""));
                let image = self.image.as_ref().unwrap();
                let extra = desc.model_column_extra.get();
                if desc.ctype == ColType::IconText {
                    let icon = (extra >= 0).then(|| item.string(extra as usize)).flatten();
                    image.set_icon_name(icon.as_deref());
                } else {
                    let file_id = if extra >= 0 { item.int64(extra as usize) } else { 0 };
                    if file_id == -2 {
                        image.clear();
                    } else {
                        image.set_from_gicon(&file_icon_for(file_id, text.as_deref()));
                    }
                }
            }
            _ => {
                let text = format_value(desc, &value);
                if let Some(label) = self.widget.downcast_ref::<gtk::EditableLabel>() {
                    if !label.is_editing() {
                        label.set_text(text.as_deref().unwrap_or(""));
                    }
                    // like a GtkTreeView, a click selects the row, and a click
                    // on a selected row starts editing
                    let selected = self.list_item.upgrade().is_some_and(|li| li.is_selected());
                    label.set_can_target(desc.editable.get() && selected);
                    label.set_editable(desc.editable.get());
                } else if let Some(label) = self.widget.downcast_ref::<gtk::Label>() {
                    label.set_text(text.as_deref().unwrap_or(""));
                }
            }
        }

        self.updating.set(false);
    }

    fn activate(&self) {
        let (Some(tv), Some(item)) = (self.tv.upgrade(), self.item.borrow().clone()) else {
            return;
        };
        let f = self.desc.activate.borrow().clone();
        if let Some(f) = f {
            f(&tv, &item);
        }
    }
}

/// Tries for a while, as a cell that was just bound isn't shown yet
fn queue_start_editing(label: &gtk::EditableLabel) {
    let tries = Cell::new(0);
    let label = label.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(20), move || {
        if !label.is_mapped() {
            tries.set(tries.get() + 1);
            if tries.get() < 20 && label.parent().is_some() {
                return glib::ControlFlow::Continue;
            }
        } else {
            label.set_can_target(true);
            label.start_editing();
            label.grab_focus();
        }
        glib::ControlFlow::Break
    });
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
        pub sort_model: RefCell<Option<gtk::SortListModel>>,
        pub selection: gtk::MultiSelection,
        pub sorter: RefCell<Option<gtk::CustomSorter>>,

        pub sort_column: Cell<i32>,
        pub sort_order: Cell<gtk::SortType>,
        pub sortable: Cell<bool>,
        pub column_menus: Cell<bool>,
        pub updating_sort: Cell<bool>,

        pub search_column: Cell<i32>,
        pub search: RefCell<String>,
        pub search_time: Cell<i64>,

        pub actions: gio::SimpleActionGroup,
        pub sorter_handler: RefCell<Option<glib::SignalHandlerId>>,
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
                sort_model: Default::default(),
                selection,
                sorter: Default::default(),
                sort_column: Cell::new(-1),
                sort_order: Cell::new(gtk::SortType::Ascending),
                sortable: Cell::new(true),
                column_menus: Cell::new(true),
                updating_sort: Cell::new(false),
                search_column: Cell::new(-1),
                search: Default::default(),
                search_time: Cell::new(0),
                actions: gio::SimpleActionGroup::new(),
                sorter_handler: Default::default(),
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
            let mut child = self.obj().first_child();
            while let Some(c) = child {
                if let Some(p) = c.downcast_ref::<gtk::Popover>() {
                    p.present();
                }
                child = c.next_sibling();
            }
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

        let weak = self.downgrade();
        let sorter = gtk::CustomSorter::new(move |a, b| {
            let Some(tv) = weak.upgrade() else {
                return gtk::Ordering::Equal;
            };
            tv.sort_func(a, b)
        });
        *imp.sorter.borrow_mut() = Some(sorter);

        let cv = &imp.column_view;
        cv.set_reorderable(true);
        cv.set_enable_rubberband(true);
        if let Some(cv_sorter) = cv.sorter() {
            let weak = self.downgrade();
            let id = cv_sorter.connect_changed(move |sorter, _| {
                if let Some(tv) = weak.upgrade() {
                    tv.column_sorter_changed(sorter);
                }
            });
            *imp.sorter_handler.borrow_mut() = Some(id);
        }
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

        let show_after =
            gio::SimpleAction::new("show-after", Some(glib::VariantTy::new("(ss)").unwrap()));
        let weak = self.downgrade();
        show_after.connect_activate(move |_, param| {
            let (Some(tv), Some((id, after_id))) =
                (weak.upgrade(), param.and_then(|p| p.get::<(String, String)>()))
            else {
                return;
            };
            let desc = tv.find_column(&id);
            let after = tv.find_column(&after_id);
            if let Some(desc) = desc.filter(|d| !d.showing()) {
                tv.add_column_after(&desc, -1, after.as_ref());
                tv.sync_sort_indicator();
                tv.update_column_menus();
            }
        });
        imp.actions.add_action(&show_after);
        self.insert_action_group("tvcol", Some(&imp.actions));

        let keys = gtk::EventControllerKey::new();
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, keyval, _, state| match weak.upgrade() {
            Some(tv) if tv.key_pressed(keyval, state) => glib::Propagation::Stop,
            _ => glib::Propagation::Proceed,
        });
        self.add_controller(keys);

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, keyval, _, state| match weak.upgrade() {
            Some(tv) if tv.expand_key_pressed(keyval, state) => glib::Propagation::Stop,
            _ => glib::Propagation::Proceed,
        });
        self.add_controller(keys);
    }

    pub fn prefs(&self) -> Option<Prefs> {
        self.imp().prefs.borrow().clone()
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

    pub fn reg_column(
        &self,
        ctype: ColType,
        model_column: i32,
        header: &str,
        id: &str,
        flags: u32,
    ) -> Rc<ColumnDesc> {
        let desc = Rc::new(ColumnDesc {
            model_column,
            model_column_extra: Cell::new(-1),
            header: header.to_string(),
            id: id.to_string(),
            flags: Cell::new(flags),
            ctype,
            activate: Default::default(),
            edit: Default::default(),
            editable: Cell::new(flags & COLUMN_EDITABLE != 0),
            column: Default::default(),
        });
        self.imp().columns.borrow_mut().push(desc.clone());
        desc
    }

    fn find_column(&self, id: &str) -> Option<Rc<ColumnDesc>> {
        self.imp().columns.borrow().iter().find(|d| d.id == id).cloned()
    }

    fn find_model_column(&self, column: i32) -> Option<Rc<ColumnDesc>> {
        self.imp().columns.borrow().iter().find(|d| d.model_column == column).cloned()
    }

    pub fn is_column_showing(&self, index: i32) -> bool {
        self.find_model_column(index).is_some_and(|d| d.showing())
    }

    /* Cells */

    fn cell_setup(&self, list_item: &gtk::ListItem, desc: &Rc<ColumnDesc>) {
        let mut image = None;
        let widget: gtk::Widget;
        let mut child: gtk::Widget;

        match desc.ctype {
            ColType::IconText | ColType::FileIconText => {
                let bx = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let img = gtk::Image::new();
                bx.append(&img);
                let label = gtk::Label::new(None);
                label.set_xalign(0.0);
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                label.set_hexpand(true);
                bx.append(&label);
                image = Some(img);
                widget = label.upcast();
                child = bx.upcast();
            }
            ColType::Wanted => {
                let check = gtk::CheckButton::new();
                check.set_halign(gtk::Align::Center);
                // the keyboard focus stays on the row, so Enter activates the
                // row rather than toggling the check box
                check.set_focusable(false);
                check.set_focus_on_click(false);
                widget = check.clone().upcast();
                child = check.upcast();
            }
            ColType::Prog => {
                let bar = gtk::ProgressBar::new();
                bar.set_show_text(true);
                bar.set_valign(gtk::Align::Center);
                widget = bar.clone().upcast();
                child = bar.upcast();
            }
            _ => {
                if desc.flags.get() & COLUMN_EDITABLE != 0 {
                    let label = gtk::EditableLabel::new("");
                    widget = label.clone().upcast();
                    child = label.upcast();
                } else {
                    let label = gtk::Label::new(None);
                    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    let xalign = match desc.ctype {
                        ColType::Speed
                        | ColType::Size
                        | ColType::Ratio
                        | ColType::NumGtZero
                        | ColType::NumGtEqZero => 1.0,
                        _ => 0.0,
                    };
                    label.set_xalign(xalign);
                    widget = label.clone().upcast();
                    child = label.upcast();
                }
            }
        }

        // the expander of a tree goes in its file name column
        let mut expander = None;
        let is_tree =
            self.imp().model.borrow().as_ref().is_some_and(|m| m.is::<gtk::TreeListModel>());
        if desc.ctype == ColType::FileIconText && is_tree {
            let e = gtk::TreeExpander::new();
            e.set_child(Some(&child));
            child = e.clone().upcast();
            expander = Some(e);
        }

        let cell = Rc::new(CellState {
            desc: desc.clone(),
            tv: self.downgrade(),
            list_item: list_item.downgrade(),
            expander,
            image,
            widget: widget.clone(),
            item: Default::default(),
            handlers: Default::default(),
            updating: Cell::new(false),
            edit_original: Default::default(),
        });

        let weak = Rc::downgrade(&cell);
        if let Some(check) = widget.downcast_ref::<gtk::CheckButton>() {
            let weak = weak.clone();
            check.connect_toggled(move |_| {
                let Some(cell) = weak.upgrade() else { return };
                if cell.updating.get() || cell.item.borrow().is_none() {
                    return;
                }
                cell.activate();
                // the model decides the state
                cell.refresh();
            });
        } else if let Some(label) = widget.downcast_ref::<gtk::EditableLabel>() {
            let w = weak.clone();
            label.connect_editing_notify(move |label| {
                if let Some(cell) = w.upgrade() {
                    cell_editing_changed(&cell, label);
                }
            });
            let w = weak.clone();
            list_item.connect_selected_notify(move |_| {
                if let Some(cell) = w.upgrade() {
                    let editing =
                        cell.widget.downcast_ref::<gtk::EditableLabel>().unwrap().is_editing();
                    if cell.item.borrow().is_some() && !editing {
                        cell.refresh();
                    }
                }
            });
        }

        if desc.ctype == ColType::Prio {
            let gesture = gtk::GestureClick::new();
            gesture.set_button(gdk::BUTTON_PRIMARY);
            let w = weak.clone();
            gesture.connect_released(move |gesture, n_press, _, _| {
                let Some(cell) = w.upgrade() else { return };
                if n_press == 1
                    && cell.item.borrow().is_some()
                    && cell.desc.activate.borrow().is_some()
                {
                    cell.activate();
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }
            });
            widget.add_controller(gesture);
        }

        mark_row_widget(&child, list_item);
        unsafe {
            list_item.set_data(DATA_CELL, cell);
        }
        list_item.set_child(Some(&child));
    }

    fn cell_bind(&self, list_item: &gtk::ListItem) {
        let Some(cell) = cell_of(list_item) else { return };
        let Some(row) = list_item.item() else { return };

        if let Some(e) = &cell.expander {
            e.set_list_row(row.downcast_ref::<gtk::TreeListRow>());
        }

        let Some(item) = item::row_item(&row) else { return };
        *cell.item.borrow_mut() = Some(item.clone());

        let mut handlers = Vec::new();
        let weak = Rc::downgrade(&cell);
        handlers.push(item.connect_column_changed(cell.desc.model_column as usize, move |_| {
            if let Some(c) = weak.upgrade() {
                c.refresh();
            }
        }));
        let extra = cell.desc.model_column_extra.get();
        if matches!(cell.desc.ctype, ColType::IconText | ColType::FileIconText) && extra >= 0 {
            let weak = Rc::downgrade(&cell);
            handlers.push(item.connect_column_changed(extra as usize, move |_| {
                if let Some(c) = weak.upgrade() {
                    c.refresh();
                }
            }));
        }
        *cell.handlers.borrow_mut() = handlers;

        cell.refresh();

        if let Some(label) = cell.widget.downcast_ref::<gtk::EditableLabel>() {
            if item.start_edit_column() == cell.desc.model_column {
                item.set_start_edit_column(-1);
                queue_start_editing(label);
            }
        }
    }

    fn cell_unbind(&self, list_item: &gtk::ListItem) {
        let Some(cell) = cell_of(list_item) else { return };
        if let Some(item) = cell.item.take() {
            for h in cell.handlers.take() {
                item.disconnect(h);
            }
        }
        if let Some(e) = &cell.expander {
            e.set_list_row(None);
        }
    }

    fn find_position(&self, item: &Item) -> Option<u32> {
        let model = &self.imp().selection;
        (0..model.n_items())
            .find(|&i| model.item(i).and_then(|r| item::row_item(&r)).as_ref() == Some(item))
    }

    fn list_base(&self) -> Option<gtk::Widget> {
        let imp = self.imp();
        if let Some(lv) = imp.list_view.borrow().as_ref() {
            if lv.parent().is_some() {
                return Some(lv.clone().upcast());
            }
        }
        // the rows of a column view are in a GtkListView
        let mut child = imp.column_view.first_child();
        while let Some(c) = child {
            if c.is::<gtk::ListView>() {
                return Some(c);
            }
            child = c.next_sibling();
        }
        None
    }

    fn scroll_to(&self, position: Option<u32>) {
        if let (Some(list), Some(position)) = (self.list_base(), position) {
            let _ = list.activate_action("list.scroll-to-item", Some(&position.to_variant()));
        }
    }

    /// Selects only the row at position, and shows it
    fn select_and_focus(&self, position: u32) {
        self.imp().selection.select_item(position, true);
        self.scroll_to(Some(position));
    }

    fn find_edit_cell(
        widget: &gtk::Widget,
        item: &Item,
        desc: &Rc<ColumnDesc>,
    ) -> Option<gtk::EditableLabel> {
        if let Some(label) = widget.downcast_ref::<gtk::EditableLabel>() {
            let cell = marked_list_item(widget).and_then(|li| cell_of(&li));
            return cell
                .filter(|c| Rc::ptr_eq(&c.desc, desc) && c.item.borrow().as_ref() == Some(item))
                .map(|_| label.clone());
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            if let Some(found) = Self::find_edit_cell(&c, item, desc) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }

    /// Starts editing the cell of item in the editable column desc, now if
    /// it's shown, or once it's shown.
    pub fn start_editing(&self, item: &Item, desc: &Rc<ColumnDesc>) {
        match Self::find_edit_cell(self.imp().column_view.upcast_ref(), item, desc) {
            Some(label) => queue_start_editing(&label),
            None => {
                item.set_start_edit_column(desc.model_column);
                self.scroll_to(self.find_position(item));
            }
        }
    }

    fn stop_editing_in(widget: &gtk::Widget) {
        if let Some(label) = widget.downcast_ref::<gtk::EditableLabel>() {
            if label.is_editing() {
                label.stop_editing(true);
            }
            return;
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            Self::stop_editing_in(&c);
            child = c.next_sibling();
        }
    }

    /// Finishes the editing of any cell, keeping its text. A cell being
    /// edited only does that itself a moment after it loses the focus.
    pub fn stop_editing(&self) {
        Self::stop_editing_in(self.imp().column_view.upcast_ref());
    }

    /* Sorting */

    fn sort_func(&self, a: &glib::Object, b: &glib::Object) -> gtk::Ordering {
        let imp = self.imp();
        let column = imp.sort_column.get();
        let (Some(a), Some(b)) = (a.downcast_ref::<Item>(), b.downcast_ref::<Item>()) else {
            return gtk::Ordering::Equal;
        };
        if column < 0 || column as usize >= a.n_columns() {
            return gtk::Ordering::Equal;
        }
        let mut result = a.compare(b, column as usize);
        if imp.sort_order.get() == gtk::SortType::Descending {
            result = result.reverse();
        }
        result.into()
    }

    /// Shows the sort column in the column headers
    fn sync_sort_indicator(&self) {
        let imp = self.imp();
        let desc = self.find_model_column(imp.sort_column.get());
        let column = desc.and_then(|d| d.column.borrow().clone());
        imp.updating_sort.set(true);
        imp.column_view.sort_by_column(column.as_ref(), imp.sort_order.get());
        imp.updating_sort.set(false);
    }

    /// Sorting by a model column, -1 for the model's order
    pub fn set_sort(&self, column: i32, order: gtk::SortType) {
        let imp = self.imp();
        let column = column.max(-1);
        if imp.sort_column.get() == column && imp.sort_order.get() == order {
            return;
        }
        imp.sort_column.set(column);
        imp.sort_order.set(order);
        self.sync_sort_indicator();
        self.resort();
        self.emit_by_name::<()>("sort-changed", &[]);
    }

    pub fn sort(&self) -> (i32, gtk::SortType) {
        let imp = self.imp();
        (imp.sort_column.get(), imp.sort_order.get())
    }

    /// The items changed, so they may be in a different order now
    pub fn resort(&self) {
        let imp = self.imp();
        if imp.sort_column.get() >= 0 {
            if let Some(s) = imp.sorter.borrow().as_ref() {
                s.changed(gtk::SorterChange::Different);
            }
        }
    }

    /// A column header was clicked
    fn column_sorter_changed(&self, sorter: &gtk::Sorter) {
        if self.imp().updating_sort.get() {
            return;
        }
        let Some(cvs) = sorter.downcast_ref::<gtk::ColumnViewSorter>() else { return };
        let desc = cvs.primary_sort_column().and_then(|c| {
            self.imp()
                .columns
                .borrow()
                .iter()
                .find(|d| d.column.borrow().as_ref() == Some(&c))
                .cloned()
        });
        match desc {
            Some(d) => self.set_sort(d.model_column, cvs.primary_sort_order()),
            None => self.set_sort(-1, gtk::SortType::Ascending),
        }
    }

    pub fn set_sortable(&self, sortable: bool) {
        self.imp().sortable.set(sortable);
    }

    /// Creates the stateful sort actions in map (a window, so "win." actions)
    /// and returns a menu using them.
    pub fn sort_menu(&self, map: &impl IsA<gio::ActionMap>) -> gio::Menu {
        let (sort, sort_type) = self.sort();
        let menu = gio::Menu::new();

        let type_action = gio::SimpleAction::new_stateful(
            SORT_TYPE_ACTION,
            Some(glib::VariantTy::INT32),
            &(sort_type.into_glib()).to_variant(),
        );
        let weak = self.downgrade();
        type_action.connect_change_state(move |_, value| {
            if let (Some(tv), Some(v)) = (weak.upgrade(), value.and_then(|v| v.get::<i32>())) {
                let (column, _) = tv.sort();
                tv.set_sort(column, unsafe { glib::translate::from_glib(v) });
            }
        });
        map.add_action(&type_action);

        let column_action = gio::SimpleAction::new_stateful(
            SORT_COLUMN_ACTION,
            Some(glib::VariantTy::INT32),
            &sort.to_variant(),
        );
        let weak = self.downgrade();
        column_action.connect_change_state(move |_, value| {
            if let (Some(tv), Some(v)) = (weak.upgrade(), value.and_then(|v| v.get::<i32>())) {
                let (_, order) = tv.sort();
                tv.set_sort(v, order);
            }
        });
        map.add_action(&column_action);

        // Keep the radio items in sync when sorting changes elsewhere, eg. by
        // clicking a column header.
        self.connect_sort_changed(move |tv| {
            let (sort, order) = tv.sort();
            type_action.set_state(&order.into_glib().to_variant());
            column_action.set_state(&sort.to_variant());
        });

        let section = gio::Menu::new();
        let item = gio::MenuItem::new(Some(&tr("Ascending")), None);
        item.set_action_and_target_value(
            Some(&format!("win.{SORT_TYPE_ACTION}")),
            Some(&gtk::SortType::Ascending.into_glib().to_variant()),
        );
        section.append_item(&item);
        let item = gio::MenuItem::new(Some(&tr("Descending")), None);
        item.set_action_and_target_value(
            Some(&format!("win.{SORT_TYPE_ACTION}")),
            Some(&gtk::SortType::Descending.into_glib().to_variant()),
        );
        section.append_item(&item);
        menu.append_section(None, &section);

        let section = gio::Menu::new();
        for desc in self.imp().columns.borrow().iter() {
            if desc.flags.get() & COLUMN_HIDE_FROM_TOP_MENU == 0 {
                let item = gio::MenuItem::new(Some(&desc.header), None);
                item.set_action_and_target_value(
                    Some(&format!("win.{SORT_COLUMN_ACTION}")),
                    Some(&desc.model_column.to_variant()),
                );
                section.append_item(&item);
            }
        }
        menu.append_section(None, &section);

        menu
    }

    /* Columns */

    fn count_showing(&self) -> usize {
        self.imp().columns.borrow().iter().filter(|d| d.showing()).count()
    }

    fn hide_column(&self, desc: &ColumnDesc) {
        let Some(column) = desc.column.take() else { return };
        self.imp().column_view.remove_column(&column);
        desc.flags.set(desc.flags.get() & !COLUMN_SHOWING);
    }

    fn column_action(&self, desc: &Rc<ColumnDesc>) -> gio::SimpleAction {
        let imp = self.imp();
        let name = format!("visible-{}", desc.id);
        if let Some(action) = imp.actions.lookup_action(&name).and_downcast::<gio::SimpleAction>() {
            return action;
        }
        let action = gio::SimpleAction::new_stateful(&name, None, &false.to_variant());
        let weak = self.downgrade();
        let d = Rc::downgrade(desc);
        action.connect_activate(move |_, _| {
            let (Some(tv), Some(desc)) = (weak.upgrade(), d.upgrade()) else { return };
            if desc.showing() {
                tv.hide_column(&desc);
                tv.sync_sort_indicator();
                tv.update_column_menus();
            }
        });
        imp.actions.add_action(&action);
        action
    }

    /// The menu of each column header has the column itself, checked, which
    /// hides it, and then the hidden columns, which show themselves after it.
    fn update_column_menus(&self) {
        let n_showing = self.count_showing();
        let columns = self.imp().columns.borrow().clone();

        for desc in &columns {
            let action = self.column_action(desc);
            let showing = desc.showing();
            action.set_state(&showing.to_variant());
            action.set_enabled(
                showing && desc.flags.get() & COLUMN_UNREMOVABLE == 0 && n_showing > 1,
            );

            let Some(column) = desc.column.borrow().clone() else { continue };

            if !self.imp().column_menus.get() {
                column.set_header_menu(None::<&gio::MenuModel>);
                continue;
            }

            let menu = gio::Menu::new();
            menu.append(Some(&desc.header), Some(&format!("tvcol.visible-{}", desc.id)));
            for hidden in columns.iter().filter(|d| !d.showing()) {
                let item = gio::MenuItem::new(Some(&hidden.header), None);
                item.set_action_and_target_value(
                    Some("tvcol.show-after"),
                    Some(&(hidden.id.as_str(), desc.id.as_str()).to_variant()),
                );
                menu.append_item(&item);
            }
            column.set_header_menu(Some(&menu));
        }
    }

    pub fn set_column_menus(&self, enabled: bool) {
        self.imp().column_menus.set(enabled);
        self.update_column_menus();
    }

    fn add_column_after(&self, desc: &Rc<ColumnDesc>, width: i64, after: Option<&Rc<ColumnDesc>>) {
        let imp = self.imp();
        let cv = &imp.column_view;
        let factory = gtk::SignalListItemFactory::new();

        let weak = self.downgrade();
        let d = desc.clone();
        factory.connect_setup(move |_, obj| {
            if let (Some(tv), Some(li)) = (weak.upgrade(), obj.downcast_ref::<gtk::ListItem>()) {
                tv.cell_setup(li, &d);
            }
        });
        let weak = self.downgrade();
        factory.connect_bind(move |_, obj| {
            if let (Some(tv), Some(li)) = (weak.upgrade(), obj.downcast_ref::<gtk::ListItem>()) {
                tv.cell_bind(li);
            }
        });
        let weak = self.downgrade();
        factory.connect_unbind(move |_, obj| {
            if let (Some(tv), Some(li)) = (weak.upgrade(), obj.downcast_ref::<gtk::ListItem>()) {
                tv.cell_unbind(li);
            }
        });

        let column = gtk::ColumnViewColumn::new(Some(&desc.header), Some(factory));
        column.set_id(Some(&desc.id));
        column.set_resizable(true);
        column.set_expand(true);
        if width > 0 {
            column.set_fixed_width(width as i32);
        }

        // The column view's sorter only shows the sort column (and makes the
        // headers clickable), the view's own sorter sorts.
        if imp.sortable.get() {
            column.set_sorter(Some(&gtk::CustomSorter::new(|_, _| gtk::Ordering::Equal)));
        }

        let columns = cv.columns();
        let mut position = columns.n_items();
        if let Some(after_col) = after.and_then(|a| a.column.borrow().clone()) {
            if let Some(i) = (0..columns.n_items())
                .find(|&i| columns.item(i).as_ref() == Some(after_col.upcast_ref()))
            {
                position = i + 1;
            }
        }

        cv.insert_column(position, &column);
        *desc.column.borrow_mut() = Some(column);
        desc.flags.set(desc.flags.get() | COLUMN_SHOWING);
    }

    pub fn remove_all_columns(&self) {
        let columns = self.imp().columns.borrow().clone();
        for desc in &columns {
            self.hide_column(desc);
        }
    }

    fn with_props<R>(&self, f: impl FnOnce(&mut serde_json::Map<String, Value>) -> R) -> Option<R> {
        let prefs = self.prefs()?;
        let id = self.imp().config_id.borrow().clone();
        Some(prefs.with_tree_view_props(&id, f))
    }

    pub fn persist(&self, flags: u32) {
        let (sort_col, sort_type) = self.sort();
        let mut ids = Vec::new();
        let mut widths = Vec::new();
        if flags & PERSIST_LAYOUT != 0 {
            let columns = self.imp().column_view.columns();
            for col in (0..columns.n_items())
                .filter_map(|i| columns.item(i).and_downcast::<gtk::ColumnViewColumn>())
            {
                let desc = self
                    .imp()
                    .columns
                    .borrow()
                    .iter()
                    .find(|d| d.column.borrow().as_ref() == Some(&col))
                    .cloned();
                if let Some(desc) = desc {
                    ids.push(json!(desc.id));
                    // The width is the one the user resized the column to, or -1
                    widths.push(json!(col.fixed_width()));
                }
            }
        }
        self.with_props(|props| {
            if flags & PERSIST_SORT != 0 {
                props.insert(prefs::KEY_TV_SORT_COL.into(), json!(sort_col));
                props.insert(prefs::KEY_TV_SORT_TYPE.into(), json!(sort_type.into_glib()));
            }
            if flags & PERSIST_LAYOUT != 0 {
                props.insert(prefs::KEY_TV_WIDTHS.into(), Value::Array(widths));
                props.insert(prefs::KEY_TV_COLUMNS.into(), Value::Array(ids));
            }
        });
    }

    pub fn restore_sort(&self) {
        let saved = self.with_props(|props| {
            match (props.get(prefs::KEY_TV_SORT_COL), props.get(prefs::KEY_TV_SORT_TYPE)) {
                (Some(c), Some(t)) => Some((prefs::value_as_i64(c), prefs::value_as_i64(t))),
                _ => None,
            }
        });
        if let Some(Some((col, t))) = saved {
            self.set_sort(col as i32, unsafe { glib::translate::from_glib(t as i32) });
        }
    }

    pub fn setup_columns(&self) {
        let saved = self
            .with_props(|props| {
                match (
                    props.get(prefs::KEY_TV_COLUMNS).and_then(Value::as_array),
                    props.get(prefs::KEY_TV_WIDTHS).and_then(Value::as_array),
                ) {
                    (Some(c), Some(w)) => Some((c.clone(), w.clone())),
                    _ => None,
                }
            })
            .flatten();

        match saved {
            None => {
                let columns = self.imp().columns.borrow().clone();
                for desc in columns.iter().filter(|d| d.flags.get() & COLUMN_EXTRA == 0) {
                    self.add_column_after(desc, -1, None);
                }
            }
            Some((columns, widths)) => {
                for (c, w) in columns.iter().zip(widths.iter()) {
                    if let Some(desc) = c.as_str().and_then(|id| self.find_column(id)) {
                        if !desc.showing() {
                            self.add_column_after(&desc, prefs::value_as_i64(w), None);
                        }
                    }
                }
            }
        }

        self.sync_sort_indicator();
        self.update_column_menus();
    }

    /* Model and selection */

    /// The model is a list model of Items, or a GtkTreeListModel of them.
    pub fn set_model(&self, model: &impl IsA<gio::ListModel>) {
        let imp = self.imp();
        let model = model.as_ref().clone();
        let sorter: gtk::Sorter = {
            let s = imp.sorter.borrow().clone().unwrap();
            if model.is::<gtk::TreeListModel>() {
                gtk::TreeListRowSorter::new(Some(s)).upcast()
            } else {
                s.upcast()
            }
        };
        *imp.model.borrow_mut() = Some(model.clone());
        let sort_model = gtk::SortListModel::new(Some(model), Some(sorter));
        imp.selection.set_model(Some(&sort_model));
        *imp.sort_model.borrow_mut() = Some(sort_model);
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
        if let Some((mut iter, first)) = gtk::BitsetIter::init_first(&bitset) {
            positions.push(first);
            while let Some(p) = iter.next() {
                positions.push(p);
            }
        }
        positions
    }

    /// The selected items, in the view's order
    pub fn selected_items(&self) -> Vec<Item> {
        let sel = &self.imp().selection;
        self.selected_positions()
            .into_iter()
            .filter_map(|p| sel.item(p).and_then(|r| item::row_item(&r)))
            .collect()
    }

    pub fn count_selected(&self) -> u32 {
        self.imp().selection.selection().size() as u32
    }

    pub fn first_selected(&self) -> Option<Item> {
        let bitset = self.imp().selection.selection();
        if bitset.is_empty() {
            return None;
        }
        self.imp().selection.item(bitset.minimum()).and_then(|r| item::row_item(&r))
    }

    pub fn select_item(&self, item: &Item) {
        if let Some(position) = self.find_position(item) {
            self.imp().selection.select_item(position, true);
            self.scroll_to(Some(position));
        }
    }

    fn list_item_for(widget: Option<gtk::Widget>, stop: &gtk::Widget) -> Option<gtk::ListItem> {
        let mut widget = widget;
        while let Some(w) = widget {
            if &w == stop {
                break;
            }
            if let Some(li) = marked_list_item(&w) {
                return Some(li);
            }
            let parent = w.parent();
            // the padding of a row (a child of the list view) is outside its
            // row widget or cells
            if parent.as_ref().is_some_and(|p| p.is::<gtk::ListView>()) {
                return Self::find_marked(&w);
            }
            widget = parent;
        }
        None
    }

    fn find_marked(widget: &gtk::Widget) -> Option<gtk::ListItem> {
        if let Some(li) = marked_list_item(widget) {
            return Some(li);
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            if let Some(li) = Self::find_marked(&c) {
                return Some(li);
            }
            child = c.next_sibling();
        }
        None
    }

    pub fn point_in_header(&self, x: f64, y: f64) -> bool {
        let cv: &gtk::Widget = self.imp().column_view.upcast_ref();
        let mut widget = self.pick(x, y, gtk::PickFlags::DEFAULT);
        while let Some(w) = widget {
            if &w == cv {
                break;
            }
            if w.css_name() == "header" && w.parent().as_ref() == Some(cv) {
                return true;
            }
            widget = w.parent();
        }
        false
    }

    /// The item of the row at x, y (in the view's coordinates), and its
    /// position
    pub fn item_at(&self, x: f64, y: f64) -> Option<(Item, u32)> {
        // cells may not be targetable, like a text cell that can't be edited
        let widget = self.pick(x, y, gtk::PickFlags::INSENSITIVE | gtk::PickFlags::NON_TARGETABLE);
        let li = Self::list_item_for(widget, self.upcast_ref())?;
        let row = li.item()?;
        Some((item::row_item(&row)?, li.position()))
    }

    /// For a context menu: select the row at x, y unless it's already
    /// selected. Returns whether there was a row.
    pub fn select_at(&self, x: f64, y: f64) -> bool {
        let Some((_, position)) = self.item_at(x, y) else {
            return false;
        };
        let sel = &self.imp().selection;
        if !sel.is_selected(position) {
            sel.select_item(position, true);
        }
        true
    }

    fn focus_widget(&self) -> Option<gtk::Widget> {
        let focus = self.root()?.focus()?;
        focus.is_ancestor(self).then_some(focus)
    }

    /// The bounds of the row with the keyboard focus, for a context menu
    /// opened from the keyboard.
    pub fn focus_rect(&self) -> Option<graphene::Rect> {
        let focus = self.focus_widget()?;
        // the focus is on the row widget, or in one of its cells
        if Self::find_marked(&focus).is_none() {
            Self::list_item_for(Some(focus.clone()), self.upcast_ref())?;
        }
        focus.compute_bounds(self)
    }

    fn tree_model(&self) -> Option<gtk::TreeListModel> {
        self.model().and_downcast::<gtk::TreeListModel>()
    }

    fn set_all_expanded(&self, expanded: bool) {
        let Some(model) = self.tree_model() else { return };
        // expanding inserts the children after the row, so they're seen too
        let mut i = 0;
        while i < model.n_items() {
            if let Some(row) = model.row(i) {
                row.set_expanded(expanded);
            }
            i += 1;
        }
    }

    pub fn expand_all(&self) {
        self.set_all_expanded(true);
    }

    pub fn collapse_all(&self) {
        self.set_all_expanded(false);
    }

    /* Type ahead search, like GtkTreeView's: typing selects the first row
     * whose search column starts with (or else contains) the typed text. */

    pub fn set_search_column(&self, column: i32) {
        self.imp().search_column.set(column);
    }

    fn search(&self, key: &str) -> bool {
        let imp = self.imp();
        let model = &imp.selection;
        let needle = glib::casefold(key).to_string();
        let column = imp.search_column.get() as usize;
        let texts: Vec<Option<String>> = (0..model.n_items())
            .map(|i| {
                model
                    .item(i)
                    .and_then(|r| item::row_item(&r))
                    .and_then(|it| it.with(column, Val::to_text))
                    .map(|t| glib::casefold(&t).to_string())
            })
            .collect();

        let found = texts
            .iter()
            .position(|t| t.as_ref().is_some_and(|t| t.starts_with(&needle)))
            .or_else(|| texts.iter().position(|t| t.as_ref().is_some_and(|t| t.contains(&needle))));

        match found {
            Some(i) => {
                self.select_and_focus(i as u32);
                true
            }
            None => false,
        }
    }

    /// The row (a GtkTreeListRow of a tree) with the keyboard focus
    fn focus_row(&self) -> Option<glib::Object> {
        let focus = self.focus_widget()?;
        let li = Self::find_marked(&focus)
            .or_else(|| Self::list_item_for(Some(focus), self.upcast_ref()))?;
        li.item()
    }

    fn expand_recursive(row: &gtk::TreeListRow) {
        row.set_expanded(true);
        let n = row.children().map(|c| c.n_items()).unwrap_or(0);
        for i in 0..n {
            if let Some(child) = row.child_row(i) {
                Self::expand_recursive(&child);
            }
        }
    }

    /// The keys of GtkTreeView (and GtkTreeExpander) to expand or collapse
    /// the row with the focus: + and Shift+Right expand, * expands all below,
    /// - and / and Shift+Left collapse.
    fn expand_key(&self, keyval: gdk::Key, state: gdk::ModifierType) -> bool {
        let Some(row) = self.focus_row().and_downcast::<gtk::TreeListRow>() else {
            return false;
        };
        if !row.is_expandable() {
            return false;
        }
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        use gdk::Key;
        match keyval {
            Key::plus | Key::KP_Add => {
                row.set_expanded(true);
                true
            }
            Key::asterisk | Key::KP_Multiply => {
                Self::expand_recursive(&row);
                true
            }
            Key::minus | Key::KP_Subtract | Key::slash | Key::KP_Divide => {
                row.set_expanded(false);
                true
            }
            Key::Right | Key::KP_Right => {
                if shift {
                    row.set_expanded(true);
                }
                shift
            }
            Key::Left | Key::KP_Left => {
                if shift {
                    row.set_expanded(false);
                }
                shift
            }
            _ => false,
        }
    }

    fn other_modifiers(state: gdk::ModifierType) -> bool {
        state.intersects(
            gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::ALT_MASK
                | gdk::ModifierType::META_MASK
                | gdk::ModifierType::SUPER_MASK,
        )
    }

    /// Before the list's own keys (Shift+arrows extend the selection)
    fn expand_key_pressed(&self, keyval: gdk::Key, state: gdk::ModifierType) -> bool {
        let imp = self.imp();
        let searching = !imp.search.borrow().is_empty()
            && glib::monotonic_time() - imp.search_time.get() <= 1_000_000;
        // unless they're part of a search
        if searching || self.tree_model().is_none() || Self::other_modifiers(state) {
            return false;
        }
        self.expand_key(keyval, state)
    }

    fn key_pressed(&self, keyval: gdk::Key, state: gdk::ModifierType) -> bool {
        let imp = self.imp();
        let Some(c) = keyval.to_unicode() else { return false };
        if imp.search_column.get() < 0 || c.is_control() || Self::other_modifiers(state) {
            return false;
        }
        let now = glib::monotonic_time();
        // a pause starts a new search
        if now - imp.search_time.get() > 1_000_000 {
            imp.search.borrow_mut().clear();
        }
        imp.search_time.set(now);
        imp.search.borrow_mut().push(c);
        let key = imp.search.borrow().clone();
        self.search(&key);
        true
    }

    /* Showing the rows in a GtkListView */

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

fn cell_editing_changed(cell: &Rc<CellState>, label: &gtk::EditableLabel) {
    let (Some(tv), Some(item)) = (cell.tv.upgrade(), cell.item.borrow().clone()) else {
        return;
    };
    let edit = cell.desc.edit.borrow().clone();

    if label.is_editing() {
        *cell.edit_original.borrow_mut() = Some(label.text().to_string());
        if let Some(edit) = edit {
            edit(&tv, &item, false, None);
        }
    } else {
        let text = label.text().to_string();
        let changed = Some(&text) != cell.edit_original.borrow().as_ref();
        if let Some(edit) = edit {
            edit(&tv, &item, true, changed.then_some(text.as_str()));
        }
        cell.edit_original.take();
        if cell.item.borrow().as_ref() == Some(&item) {
            cell.refresh();
        }
    }
}

use gtk::glib::translate::IntoGlib;
