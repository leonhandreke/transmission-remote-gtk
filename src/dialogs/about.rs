//! The about dialog (trg-about-window.c)

use gtk::prelude::*;

pub fn about_window_new(parent: Option<&gtk::Window>) -> gtk::AboutDialog {
    let builder =
        gtk::Builder::from_resource("/io/github/TransmissionRemoteGtk/ui/trg-about-window.ui");
    let dialog: gtk::AboutDialog = builder.object("about_dialog").unwrap();

    dialog.set_transient_for(parent);
    dialog.set_logo_icon_name(Some(env!("TRG_PACKAGE_NAME")));
    dialog.set_program_name(Some(env!("TRG_PACKAGE_NAME")));
    dialog.set_version(Some(env!("CARGO_PKG_VERSION")));

    dialog
}
