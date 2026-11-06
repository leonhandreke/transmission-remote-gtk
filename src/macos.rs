//! macOS support (from main.c and trg-gtk-app.c): finding GTK's data and our
//! translations in an application bundle or the prefix, and following the
//! system's light/dark appearance.

use std::path::{Path, PathBuf};

/// The Resources directory of the application bundle we're run from
pub fn bundle_resources_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    if !exe_dir.ends_with("Contents/MacOS") {
        return None;
    }
    let resources = exe_dir.join("../Resources");
    Some(std::fs::canonicalize(&resources).unwrap_or(resources))
}

fn prepend_data_dir(dir: &Path) {
    let current = std::env::var("XDG_DATA_DIRS").ok().filter(|s| !s.is_empty());
    let data_dirs = current.unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let dir = dir.to_string_lossy();
    if data_dirs.split(':').any(|d| d == dir) {
        return;
    }
    // SAFETY: called at the start of main, before any other thread exists
    unsafe { std::env::set_var("XDG_DATA_DIRS", format!("{dir}:{data_dirs}")) };
}

/// Make GTK find its data (icons, GSettings schemas, ...). This has to
/// happen before GLib reads the environment.
///
/// A self-contained bundle (made by macos/bundle.py) carries its own
/// libraries and data, in Contents/Frameworks and Contents/Resources/{lib,
/// share}. Otherwise, use the data in our prefix (usually Homebrew's), which
/// isn't in the default XDG_DATA_DIRS, plus the bundle's own data if there is
/// any.
pub fn setup_environment() {
    let resources = bundle_resources_dir();
    let libdir = resources.as_ref().map(|r| r.join("lib"));
    let datadir = resources.as_ref().map(|r| r.join("share"));

    if let (Some(resources), Some(libdir), Some(datadir)) = (&resources, &libdir, &datadir)
        && libdir.is_dir()
    {
        let pixbuf_cache = libdir.join("gdk-pixbuf-2.0").join("2.10.0").join("loaders.cache");
        let gio_modules = libdir.join("gio").join("modules");
        // SAFETY: called at the start of main, before any other thread exists
        unsafe {
            std::env::set_var("XDG_DATA_DIRS", datadir);
            std::env::set_var("GDK_PIXBUF_MODULE_FILE", pixbuf_cache);
            std::env::set_var("GIO_MODULE_DIR", gio_modules);
            std::env::set_var("GTK_DATA_PREFIX", resources);
            std::env::set_var("GTK_EXE_PREFIX", resources);
        }
        return;
    }

    prepend_data_dir(Path::new(env!("TRG_DATADIR")));
    if let Some(datadir) = datadir.filter(|d| d.is_dir()) {
        prepend_data_dir(&datadir);
    }
}

/// The locale directory of a bundle, if it has one
pub fn bundle_locale_dir() -> Option<PathBuf> {
    bundle_resources_dir().map(|r| r.join("share").join("locale")).filter(|d| d.is_dir())
}

#[cfg(target_os = "macos")]
mod appearance {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::string::CFString;
    use core_foundation_sys::dictionary::CFDictionaryRef;
    use core_foundation_sys::notification_center::{
        CFNotificationCenterAddObserver, CFNotificationCenterGetDistributedCenter,
        CFNotificationCenterRef, CFNotificationName,
        CFNotificationSuspensionBehaviorDeliverImmediately,
    };
    use core_foundation_sys::preferences::{
        CFPreferencesAppSynchronize, CFPreferencesCopyAppValue, kCFPreferencesAnyApplication,
    };
    use gtk::glib;
    use gtk::prelude::*;
    use std::ffi::c_void;

    /// AppleInterfaceStyle is "Dark" in dark mode
    fn is_dark() -> bool {
        let key = CFString::from_static_string("AppleInterfaceStyle");
        unsafe {
            CFPreferencesAppSynchronize(kCFPreferencesAnyApplication);
            let value =
                CFPreferencesCopyAppValue(key.as_concrete_TypeRef(), kCFPreferencesAnyApplication);
            if value.is_null() {
                return false;
            }
            CFType::wrap_under_create_rule(value)
                .downcast::<CFString>()
                .is_some_and(|s| s == "Dark")
        }
    }

    pub fn update_color_scheme() {
        let dark = is_dark();
        let Some(settings) = gtk::Settings::default() else { return };
        // gtk-interface-color-scheme is new in GTK 4.20
        if gtk::check_version(4, 20, 0).is_none()
            && let Some(pspec) = settings.find_property("gtk-interface-color-scheme")
            && let Some(class) = glib::EnumClass::with_type(pspec.value_type())
            && let Some(value) = class.to_value_by_nick(if dark { "dark" } else { "light" })
        {
            settings.set_property_from_value("gtk-interface-color-scheme", &value);
            return;
        }
        settings.set_gtk_application_prefer_dark_theme(dark);
    }

    extern "C" fn appearance_changed(
        _center: CFNotificationCenterRef,
        _observer: *mut c_void,
        _name: CFNotificationName,
        _object: *const c_void,
        _user_info: CFDictionaryRef,
    ) {
        // A panic mustn't unwind into Core Foundation
        let _ = std::panic::catch_unwind(update_color_scheme);
    }

    /// GTK's macOS backend doesn't pass on the system's light/dark
    /// appearance, so follow it here. Changes are announced with a
    /// distributed notification.
    pub fn follow() {
        update_color_scheme();
        let name = CFString::from_static_string("AppleInterfaceThemeChangedNotification");
        unsafe {
            CFNotificationCenterAddObserver(
                CFNotificationCenterGetDistributedCenter(),
                std::ptr::null(),
                appearance_changed,
                name.as_concrete_TypeRef(),
                std::ptr::null(),
                CFNotificationSuspensionBehaviorDeliverImmediately,
            );
        }
        // observed for the lifetime of the application
        std::mem::forget(name);
    }
}

#[cfg(target_os = "macos")]
pub use appearance::follow as follow_appearance;

#[cfg(not(target_os = "macos"))]
pub fn follow_appearance() {}
