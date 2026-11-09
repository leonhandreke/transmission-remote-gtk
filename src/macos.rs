//! macOS support (from main.c and trg-gtk-app.c): finding GTK's data and our
//! translations in an application bundle or the prefix, following the
//! system's light/dark appearance, and a few AppKit conventions.

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

/// A few AppKit calls, through the Objective-C runtime, for what GTK doesn't
/// do itself
#[cfg(target_os = "macos")]
mod appkit {
    use gtk::glib::translate::ToGlibPtr;
    use gtk::prelude::*;
    use gtk::{gdk, glib};
    use std::ffi::{CStr, c_char, c_void};

    type Id = *mut c_void;
    type Sel = *const c_void;

    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> Id;
        fn sel_registerName(name: *const c_char) -> Sel;
        // called through a pointer of the method's type
        fn objc_msgSend();
    }

    unsafe extern "C" {
        // GTK 4.8
        fn gdk_macos_surface_get_native_window(surface: *mut gdk::ffi::GdkSurface) -> Id;
    }

    const NS_WINDOW_STYLE_MASK_MINIATURIZABLE: usize = 1 << 2;
    const NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_PRIMARY: usize = 1 << 7;
    const NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_NONE: usize = 1 << 9;

    unsafe fn class(name: &CStr) -> Id {
        unsafe { objc_getClass(name.as_ptr()) }
    }

    unsafe fn send<R>(receiver: Id, selector: &CStr) -> R {
        unsafe {
            let f: unsafe extern "C" fn(Id, Sel) -> R =
                std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
            f(receiver, sel_registerName(selector.as_ptr()))
        }
    }

    unsafe fn send1<A, R>(receiver: Id, selector: &CStr, arg: A) -> R {
        unsafe {
            let f: unsafe extern "C" fn(Id, Sel, A) -> R =
                std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
            f(receiver, sel_registerName(selector.as_ptr()), arg)
        }
    }

    /// macOS offers to merge windows into tabs, with Show Tab Bar and Show
    /// All Tabs in the View menu, but none of our windows have tabs. This has
    /// to happen before the windows are made.
    pub fn disable_window_tabbing() {
        unsafe {
            let ns_window = class(c"NSWindow");
            if !ns_window.is_null() {
                send1::<bool, ()>(ns_window, c"setAllowsAutomaticWindowTabbing:", false);
            }
        }
    }

    /// Dims the minimize and zoom buttons of a realized window that GTK has
    /// made non-resizable, as settings windows have them: it can't be
    /// minimized, and it doesn't go full screen (which the zoom button would
    /// still offer). GTK only ever changes the resizable flag of the style
    /// mask after making the NSWindow, and not the collection behavior.
    pub fn make_fixed(window: &impl IsA<gtk::Native>) {
        let Some(surface) = window.surface() else { return };
        if !glib::Type::from_name("GdkMacosSurface").is_some_and(|t| surface.type_().is_a(t)) {
            return;
        }
        let surface: *mut gdk::ffi::GdkSurface = surface.to_glib_none().0;
        unsafe {
            let ns_window = gdk_macos_surface_get_native_window(surface);
            if ns_window.is_null() {
                return;
            }
            let mask: usize = send(ns_window, c"styleMask");
            let behavior: usize = send(ns_window, c"collectionBehavior");
            send1::<usize, ()>(
                ns_window,
                c"setStyleMask:",
                mask & !NS_WINDOW_STYLE_MASK_MINIATURIZABLE,
            );
            send1::<usize, ()>(
                ns_window,
                c"setCollectionBehavior:",
                (behavior & !NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_PRIMARY)
                    | NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_NONE,
            );
        }
    }

    /// Makes the last menu of the menu bar the application's Help menu, with
    /// the system's search field. GTK only marks the Window menu, and AppKit
    /// finds a Help menu by itself only by its English title.
    pub fn set_help_menu() {
        unsafe {
            let app: Id = send(class(c"NSApplication"), c"sharedApplication");
            let main_menu: Id = send(app, c"mainMenu");
            if main_menu.is_null() {
                return;
            }
            let count: isize = send(main_menu, c"numberOfItems");
            if count < 1 {
                return;
            }
            let item: Id = send1(main_menu, c"itemAtIndex:", count - 1);
            let menu: Id = send(item, c"submenu");
            if !menu.is_null() {
                send1::<Id, ()>(app, c"setHelpMenu:", menu);
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use appkit::{disable_window_tabbing, make_fixed, set_help_menu};

#[cfg(not(target_os = "macos"))]
pub fn disable_window_tabbing() {}

#[cfg(not(target_os = "macos"))]
pub fn make_fixed(_window: &impl gtk::prelude::IsA<gtk::Native>) {}

#[cfg(not(target_os = "macos"))]
pub fn set_help_menu() {}
