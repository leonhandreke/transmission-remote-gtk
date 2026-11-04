//! Compiles the user interface (Blueprint files to GtkBuilder XML) and bundles
//! it, with the icons, into GResources that main.rs registers. Also passes on
//! the configuration (which meson sets for installed builds), embeds the
//! Info.plist on macOS and links libintl where the gettext functions aren't
//! in libc.
//!
//! A plain `cargo build` works for development: the translations are then
//! compiled into the build directory, and the data of GTK's prefix is used.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PACKAGE_NAME: &str = "transmission-remote-gtk";
const APPLICATION_ID: &str = "io.github.TransmissionRemoteGtk";
const PRETTY_NAME: &str = "Transmission Remote GTK";

fn pkg_config(args: &[&str]) -> Option<String> {
    let out = Command::new(env::var("PKG_CONFIG").unwrap_or_else(|_| "pkg-config".into()))
        .args(args)
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn blueprints(out_dir: &Path) -> PathBuf {
    let ui_dir = out_dir.join("ui");
    fs::create_dir_all(&ui_dir).unwrap();

    let mut files: Vec<PathBuf> = fs::read_dir("src")
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "blp"))
        .collect();
    files.sort();

    println!("cargo:rerun-if-env-changed=BLUEPRINT_COMPILER");
    let compiler = env::var("BLUEPRINT_COMPILER").unwrap_or_else(|_| "blueprint-compiler".into());
    // The compiler may be a command line (eg. "python3 /path/to/blueprint-compiler.py")
    let mut words = compiler.split_whitespace();
    let mut cmd = Command::new(words.next().unwrap());
    cmd.args(words);
    let status = cmd
        .arg("batch-compile")
        .arg(&ui_dir)
        .arg("src")
        .args(&files)
        .status()
        .expect("blueprint-compiler (>= 0.16) is needed to build the user interface");
    assert!(status.success(), "blueprint-compiler failed");

    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
    }

    ui_dir
}

fn resources(ui_dir: &Path) {
    let ui_dir = ui_dir.to_str().unwrap();
    glib_build_tools::compile_resources(&[ui_dir], "src/trg.gresource.xml", "trg-ui.gresource");
    glib_build_tools::compile_resources(
        &["data"],
        &format!("data/{APPLICATION_ID}.gresource.xml"),
        "trg-data.gresource",
    );
}

/// Translations for running from the build directory
fn compile_translations(out_dir: &Path) -> PathBuf {
    let locale_dir = out_dir.join("locale");
    println!("cargo:rerun-if-changed=po/LINGUAS");
    let Ok(linguas) = fs::read_to_string("po/LINGUAS") else {
        return locale_dir;
    };
    for lang in linguas.split_whitespace().filter(|l| !l.starts_with('#')) {
        let po = format!("po/{lang}.po");
        println!("cargo:rerun-if-changed={po}");
        let dir = locale_dir.join(lang).join("LC_MESSAGES");
        fs::create_dir_all(&dir).unwrap();
        let status = Command::new("msgfmt")
            .arg("-o")
            .arg(dir.join(format!("{PACKAGE_NAME}.mo")))
            .arg(&po)
            .status();
        if !matches!(status, Ok(s) if s.success()) {
            println!("cargo:warning=msgfmt failed for {po}, its translations will be missing");
        }
    }
    locale_dir
}

/// GTK's prefix. Homebrew's pkg-config files give the package's own Cellar
/// directory, but the data of all packages is linked into Homebrew's prefix.
fn gtk_prefix() -> String {
    let prefix = pkg_config(&["--variable=prefix", "gtk4"]).unwrap_or_else(|| "/usr".into());
    match prefix.find("/Cellar/") {
        Some(i) => prefix[..i].to_string(),
        None => prefix,
    }
}

fn config(out_dir: &Path) {
    for var in ["TRG_LOCALEDIR", "TRG_DATADIR"] {
        println!("cargo:rerun-if-env-changed={var}");
    }

    let locale_dir = match env::var("TRG_LOCALEDIR") {
        Ok(dir) => dir,
        Err(_) => compile_translations(out_dir).display().to_string(),
    };
    let data_dir = env::var("TRG_DATADIR").unwrap_or_else(|_| format!("{}/share", gtk_prefix()));

    println!("cargo:rustc-env=TRG_PACKAGE_NAME={PACKAGE_NAME}");
    println!("cargo:rustc-env=TRG_APPLICATION_ID={APPLICATION_ID}");
    println!("cargo:rustc-env=TRG_LOCALEDIR={locale_dir}");
    println!("cargo:rustc-env=TRG_DATADIR={data_dir}");
}

/// The gettext functions are in libc with glibc, elsewhere in libintl, which
/// GLib links to.
fn link_libintl() {
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") {
        return;
    }
    let libs = pkg_config(&["--libs", "--static", "glib-2.0"]).unwrap_or_default();
    let mut found = false;
    for dir in libs.split_whitespace().filter_map(|l| l.strip_prefix("-L")) {
        if ["libintl.dylib", "libintl.so", "libintl.a", "libintl.dll.a"]
            .iter()
            .any(|f| Path::new(dir).join(f).exists())
        {
            println!("cargo:rustc-link-search=native={dir}");
            found = true;
        }
    }
    if found || libs.split_whitespace().any(|l| l == "-lintl") {
        println!("cargo:rustc-link-lib=intl");
    }
}

/// macOS: embed the Info.plist (also used for the application bundle) in the
/// executable, so it has the right name in the application menu even when it
/// isn't run from a bundle.
fn info_plist(out_dir: &Path) {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    println!("cargo:rerun-if-changed=macos/Info.plist.in");
    let plist = fs::read_to_string("macos/Info.plist.in")
        .unwrap()
        .replace("@PRETTY_NAME@", PRETTY_NAME)
        .replace("@APP_ID@", APPLICATION_ID)
        .replace("@PROJECT_NAME@", PACKAGE_NAME)
        .replace("@VERSION@", &env::var("CARGO_PKG_VERSION").unwrap());
    let path = out_dir.join("Info.plist");
    fs::write(&path, plist).unwrap();
    println!("cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}", path.display());
    // Room for macos/bundle.py to rewrite the install names (meson adds
    // this for C executables)
    println!("cargo:rustc-link-arg-bins=-Wl,-headerpad_max_install_names");
}

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let ui_dir = blueprints(&out_dir);
    resources(&ui_dir);
    config(&out_dir);
    link_libintl();
    info_plist(&out_dir);
}
