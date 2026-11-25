//! The dialogs of the application.

use gtk::prelude::*;

pub mod about;
pub mod add;
pub mod add_url;
pub mod destination_combo;
pub mod dialog;
pub mod file_rename;
pub mod json_widgets;
pub mod move_dialog;
pub mod preferences;
pub mod props;
pub mod remote_prefs;
pub mod settings;
pub mod stats;

/// The open window of type T, for the dialogs there's only one of, or else
/// a new one
pub fn single<T: IsA<gtk::Window> + IsA<gtk::glib::Object>>(new: impl FnOnce() -> T) -> T {
    let windows = gtk::Window::toplevels();
    (0..windows.n_items()).find_map(|i| windows.item(i).and_downcast::<T>()).unwrap_or_else(new)
}
