//! Offscreen snapshots for UI debugging, built only with the debug-snapshot
//! cargo feature (cargo build --features debug-snapshot) and enabled with
//! TRG_DEBUG_SNAPSHOT=<dir>. They work without a screen, for example while
//! it's locked and screenshots are black.
//!
//! When the file <dir>/request appears (it's polled twice a second), each
//! of its lines is activated as an action on the main window
//! ("win.view-notebook", "app.quit", or with a parameter in GVariant text
//! format, "win.connect 0"), the file is removed, and then every visible
//! window and popover is rendered to <dir>/<n>-<title>[-popover-<m>].png at
//! its scale factor. The names of the files are written to <dir>/index.txt,
//! the last thing written. With TRG_DEBUG_SNAPSHOT_INTERVAL=<seconds>, the
//! windows are also rendered periodically.

use gtk::prelude::*;
use gtk::{glib, graphene};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn init(app: &gtk::Application) {
    let Some(dir) = std::env::var_os("TRG_DEBUG_SNAPSHOT").map(PathBuf::from) else { return };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("debug-snapshot: {}: {e}", dir.display());
        return;
    }
    eprintln!("debug-snapshot: write {} to render the windows", dir.join("request").display());

    let app = app.downgrade();
    let d = dir.clone();
    glib::timeout_add_local(Duration::from_millis(500), move || {
        let Some(app) = app.upgrade() else { return glib::ControlFlow::Break };
        let request = d.join("request");
        if let Ok(text) = std::fs::read_to_string(&request) {
            let _ = std::fs::remove_file(&request);
            run_actions(&app, &text);
            // let the actions take effect and the windows lay themselves out
            let d = d.clone();
            glib::timeout_add_local_once(Duration::from_millis(300), move || snapshot_all(&d));
        }
        glib::ControlFlow::Continue
    });

    let interval = std::env::var("TRG_DEBUG_SNAPSHOT_INTERVAL").ok().and_then(|s| s.parse().ok());
    if let Some(secs) = interval.filter(|s: &f64| *s > 0.0) {
        glib::timeout_add_local(Duration::from_secs_f64(secs), move || {
            snapshot_all(&dir);
            glib::ControlFlow::Continue
        });
    }
}

fn run_actions(app: &gtk::Application, text: &str) {
    let windows = app.windows();
    let main = windows.iter().find(|w| w.is::<crate::main_window::MainWindow>());
    let Some(win) = main.or(windows.first()) else { return };
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (name, param) = line.split_once(' ').unwrap_or((line, ""));
        let param = match param.trim() {
            "" => None,
            p => match glib::Variant::parse(None, p) {
                Ok(v) => Some(v),
                Err(e) => {
                    eprintln!("debug-snapshot: {line}: {e}");
                    continue;
                }
            },
        };
        match win.activate_action(name, param.as_ref()) {
            Ok(()) => eprintln!("debug-snapshot: activated {line}"),
            Err(e) => eprintln!("debug-snapshot: {line}: {e}"),
        }
    }
}

fn snapshot_all(dir: &Path) {
    let mut names = Vec::new();
    let windows = gtk::Window::list_toplevels();
    for (n, w) in windows.iter().filter(|w| w.is_visible() && w.is_mapped()).enumerate() {
        let title = w.downcast_ref::<gtk::Window>().and_then(|w| w.title()).unwrap_or_default();
        let base = format!("{n}-{}", sanitize(&title));
        save(w, dir, &format!("{base}.png"), &mut names);
        let mut popovers = Vec::new();
        find_popovers(w, &mut popovers);
        for (m, p) in popovers.iter().enumerate() {
            save(p, dir, &format!("{base}-popover-{m}.png"), &mut names);
        }
    }
    let index = names.join("\n") + "\n";
    if let Err(e) = std::fs::write(dir.join("index.txt"), index) {
        eprintln!("debug-snapshot: index.txt: {e}");
    }
}

fn find_popovers(w: &gtk::Widget, out: &mut Vec<gtk::Widget>) {
    let mut child = w.first_child();
    while let Some(c) = child {
        if c.is::<gtk::Popover>() && c.is_mapped() {
            out.push(c.clone());
        }
        find_popovers(&c, out);
        child = c.next_sibling();
    }
}

fn sanitize(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .take(40)
        .collect();
    if s.is_empty() { "untitled".into() } else { s }
}

fn save(w: &gtk::Widget, dir: &Path, name: &str, names: &mut Vec<String>) {
    let (width, height) = (w.width(), w.height());
    let Some(renderer) = w.native().and_then(|n| n.renderer()) else { return };
    if width <= 0 || height <= 0 {
        return;
    }
    let scale = w.scale_factor() as f32;
    let snapshot = gtk::Snapshot::new();
    snapshot.scale(scale, scale);
    gtk::WidgetPaintable::new(Some(w)).snapshot(&snapshot, width as f64, height as f64);
    let Some(node) = snapshot.to_node() else { return };
    let viewport = graphene::Rect::new(0.0, 0.0, width as f32 * scale, height as f32 * scale);
    let texture = renderer.render_texture(&node, Some(&viewport));
    // write to a temporary file first, so that readers never see half a file
    let tmp = dir.join(format!(".{name}"));
    match texture.save_to_png(&tmp).map_err(|e| e.to_string()) {
        Ok(()) => {
            let _ = std::fs::rename(&tmp, dir.join(name));
            names.push(name.to_string());
        }
        Err(e) => eprintln!("debug-snapshot: {name}: {e}"),
    }
}
