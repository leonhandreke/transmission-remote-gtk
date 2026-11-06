//! Formatting (sizes, speeds, times, ratios, from Transmission), URL checks
//! and GTK helpers: error dialogs, context menus and popover menus.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use crate::client::Error;
use crate::i18n::{ctr, ntr, qtr, sprintf, tr};

/* Sizes and speeds, as Transmission formats them */

pub const DISK_K: i64 = 1024;
pub const SPEED_K: i64 = 1024;

fn size_units() -> [String; 4] {
    [tr("KiB"), tr("MiB"), tr("GiB"), tr("TiB")]
}

fn speed_units() -> [String; 4] {
    [tr("KiB/s"), tr("MiB/s"), tr("GiB/s"), tr("TiB/s")]
}

/// tr_formatter_size_B()
pub fn format_size(bytes: i64) -> String {
    let units = size_units();
    let k = DISK_K as f64;
    let values = [k, k * k, k * k * k, k * k * k * k];
    let b = bytes as f64;
    let i = if b < values[1] {
        0
    } else if b < values[2] {
        1
    } else if b < values[3] {
        2
    } else {
        3
    };
    let value = b / values[i];
    let precision = if value < 100.0 { 2 } else { 1 };
    format!("{value:.precision$} {}", units[i])
}

/// tr_formatter_speed_KBps()
pub fn format_speed_kbps(kbps: f64) -> String {
    let units = speed_units();
    let k = SPEED_K as f64;
    let mut speed = kbps;
    if speed <= 999.95 {
        format!("{} {}", speed as i64, units[0])
    } else {
        speed /= k;
        if speed <= 99.995 {
            format!("{speed:.2} {}", units[1])
        } else if speed <= 999.95 {
            format!("{speed:.1} {}", units[1])
        } else {
            speed /= k;
            format!("{speed:.1} {}", units[2])
        }
    }
}

/// A speed in bytes per second
pub fn format_speed(bytes_per_second: i64) -> String {
    format_speed_kbps((bytes_per_second / DISK_K) as f64)
}

/// tr_strlsize(): "None" for 0
pub fn format_size_or_none(bytes: i64) -> String {
    if bytes == 0 { qtr("None") } else { format_size(bytes) }
}

pub fn truncd(x: f64, decimal_places: i32) -> f64 {
    let i = 10f64.powi(decimal_places);
    ((x * i) as i64) as f64 / i
}

/// tr_strlpercent()
pub fn format_percent(x: f64) -> String {
    let precision = if x < 10.0 {
        2
    } else if x < 100.0 {
        1
    } else {
        0
    };
    format!("{:.*}%", precision as usize, truncd(x, precision))
}

/// tr_strlratio()
pub fn format_ratio(ratio: f64) -> String {
    if ratio.is_nan() {
        tr("None")
    } else if ratio.is_infinite() {
        "\u{221E}".into()
    } else if ratio < 10.0 {
        format!("{:.2}", truncd(ratio, 2))
    } else if ratio < 100.0 {
        format!("{:.1}", truncd(ratio, 1))
    } else {
        format!("{ratio:.0}")
    }
}

/// tr_strltime_short(): hh:mm:ss
pub fn format_time_short(seconds: i64) -> String {
    let s = seconds.max(0);
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, (s % 3600) % 60)
}

/// tr_strltime_long(): "2 days, 3 hours"
pub fn format_time_long(seconds: i64) -> String {
    let s = seconds.max(0);
    let days = s / 86400;
    let hours = (s % 86400) / 3600;
    let minutes = (s % 3600) / 60;
    let secs = (s % 3600) % 60;

    let d = sprintf(&ntr("%d day", "%d days", days), &[&days]);
    let h = sprintf(&ntr("%d hour", "%d hours", hours), &[&hours]);
    let m = sprintf(&ntr("%d minute", "%d minutes", minutes), &[&minutes]);
    let sec = sprintf(&ntr("%ld second", "%ld seconds", secs), &[&secs]);

    if days > 0 {
        if days >= 4 || hours == 0 { d } else { format!("{d}, {h}") }
    } else if hours > 0 {
        if hours >= 4 || minutes == 0 { h } else { format!("{h}, {m}") }
    } else if minutes > 0 {
        if minutes >= 4 || secs == 0 { m } else { format!("{m}, {sec}") }
    } else {
        sec
    }
}

/// "%F %H:%M:%S" in local time, "N/A" for 0
pub fn epoch_to_string(epoch: i64) -> String {
    if epoch == 0 {
        return tr("N/A");
    }
    glib::DateTime::from_unix_local(epoch)
        .and_then(|dt| dt.format("%F %H:%M:%S"))
        .map(|s| s.to_string())
        .unwrap_or_default()
}

/// The progress bar text of a percentage
pub fn progress_text(progress: f64) -> String {
    sprintf(&ctr("progress bar label", "%d %%"), &[&(progress as i64)])
}

/* URLs */

pub fn is_magnet(s: &str) -> bool {
    s.starts_with("magnet:")
}

pub fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// The host of a tracker URL, without www., tracker. and the like
pub struct HostRegex(glib::Regex);

impl Default for HostRegex {
    fn default() -> Self {
        Self::new()
    }
}

impl HostRegex {
    pub fn new() -> Self {
        Self(
            glib::Regex::new(
                r"^[^:/?#]+:?//(?:www\.|torrent\.|torrents\.|tracker\.|\d+\.)?([^/?#:]*)",
                glib::RegexCompileFlags::OPTIMIZE,
                glib::RegexMatchFlags::empty(),
            )
            .ok()
            .flatten()
            .expect("the host regex is valid"),
        )
    }

    /// The first group of the match, like trg_gregex_get_first()
    pub fn first(&self, src: &str) -> Option<String> {
        let s = glib::GString::from(src);
        let mi = self.0.match_(s.as_gstr(), glib::RegexMatchFlags::empty()).ok()?;
        if mi.matches() { mi.fetch(1).map(|s| s.to_string()) } else { None }
    }
}

pub fn base64_encode_file(filename: &str) -> Result<String, String> {
    use base64::Engine;
    let data = std::fs::read(filename).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(data))
}

pub fn rm_trailing_slashes(s: &str) -> &str {
    s.trim_end_matches('/')
}

/// Wraps the (first) link in text with a hyperlink, for Pango markup. The
/// result is always escaped markup.
pub fn add_links_to_text(original: &str) -> String {
    let lower = original.to_ascii_lowercase();
    if lower.contains("<a ") {
        return original.to_string();
    }
    let mut out = String::new();
    let mut rest = original;
    while let Some(i) = rest.find("http://").or_else(|| rest.find("https://")).map(|i| {
        // the earlier of both
        match (rest.find("http://"), rest.find("https://")) {
            (Some(a), Some(b)) => a.min(b),
            _ => i,
        }
    }) {
        out.push_str(&glib::markup_escape_text(&rest[..i]));
        let url_len = rest[i..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || "_-./?=&:".contains(c)))
            .unwrap_or(rest.len() - i);
        let url = &rest[i..i + url_len];
        let escaped = glib::markup_escape_text(url);
        out.push_str(&format!("<a href='{escaped}'>{escaped}</a>"));
        rest = &rest[i + url_len..];
    }
    out.push_str(&glib::markup_escape_text(rest));
    out
}

/* GTK */

/// Whether we're running on GTK's macOS backend, so should follow macOS
/// conventions (native menu bar, Command key shortcuts, ...).
pub fn is_macos() -> bool {
    cfg!(target_os = "macos")
        && gdk::Display::default().is_some_and(|d| d.type_().name() == "GdkMacosDisplay")
}

/// The primary shortcut modifier: Command on macOS, Control elsewhere
pub fn primary_modifier() -> gdk::ModifierType {
    if is_macos() { gdk::ModifierType::META_MASK } else { gdk::ModifierType::CONTROL_MASK }
}

/// Shows an error message, calls done (if any) once it's dismissed.
pub fn error_dialog_full(
    parent: Option<&impl IsA<gtk::Window>>,
    msg: &str,
    done: Option<Box<dyn FnOnce()>>,
) {
    let dialog = gtk::AlertDialog::builder().message(tr("Error")).detail(msg).modal(true).build();
    dialog.choose(parent, gio::Cancellable::NONE, move |_| {
        if let Some(done) = done {
            done();
        }
    });
}

pub fn error_dialog(parent: Option<&impl IsA<gtk::Window>>, msg: &str) {
    error_dialog_full(parent, msg, None);
}

pub fn client_error_dialog(parent: Option<&impl IsA<gtk::Window>>, error: &Error) {
    error_dialog(parent, &error.to_string());
}

/// Context menus: a click gesture for the secondary button and a shortcut for
/// the keyboard (Shift+F10 and the Menu key). x and y are -1 when the menu
/// was requested from the keyboard. func returns whether it showed a menu.
pub fn add_context_menu<W: IsA<gtk::Widget>>(
    widget: &W,
    func: impl Fn(&gtk::Widget, f64, f64) -> bool + 'static,
) {
    let func = Rc::new(func);

    let gesture = gtk::GestureClick::new();
    gesture.set_button(gdk::BUTTON_SECONDARY);
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
    let f = func.clone();
    gesture.connect_pressed(move |gesture, n_press, x, y| {
        if n_press != 1 {
            return;
        }
        if let Some(widget) = gesture.widget()
            && f(&widget, x, y)
        {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    widget.add_controller(gesture);

    let shortcuts = gtk::ShortcutController::new();
    let f = func.clone();
    shortcuts.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("<Shift>F10|Menu"),
        Some(gtk::CallbackAction::new(move |widget, _| {
            if f(widget, -1.0, -1.0) { glib::Propagation::Stop } else { glib::Propagation::Proceed }
        })),
    ));
    // capture, so the widget's own key handling can't swallow it
    shortcuts.set_propagation_phase(gtk::PropagationPhase::Capture);
    widget.add_controller(shortcuts);
}

/// Pops up a context menu for model on parent, at x/y (-1 for the whole
/// widget). actions (if any) are inserted with prefix for the items.
pub fn popover_menu_popup(
    parent: &impl IsA<gtk::Widget>,
    model: &impl IsA<gio::MenuModel>,
    mut x: f64,
    mut y: f64,
    prefix: &str,
    actions: Option<&impl IsA<gio::ActionGroup>>,
) {
    let parent = parent.as_ref();
    // nested submenus, like GTK 3 menus, rather than sliding ones
    let popover = gtk::PopoverMenu::from_model_full(model, gtk::PopoverMenuFlags::NESTED);

    if let Some(actions) = actions {
        popover.insert_action_group(prefix, Some(actions));
    }

    // A tree view presents its popovers. From the keyboard, point at the row
    // with the focus if there is one.
    if (x < 0.0 || y < 0.0)
        && let Some(tv) = parent.downcast_ref::<crate::tree_view::TreeView>()
        && let Some(bounds) = tv.focus_rect()
    {
        x = bounds.x() as f64 + 20.0;
        y = bounds.y() as f64 + bounds.height() as f64 / 2.0;
    }

    popover.set_parent(parent);
    popover.set_has_arrow(false);
    popover.set_halign(gtk::Align::Start);

    if x >= 0.0 && y >= 0.0 {
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    }

    popover.connect_closed(|popover| {
        // Defer, the activated item's action is dispatched after "closed".
        let popover = popover.clone();
        glib::idle_add_local_once(move || {
            if popover.parent().is_some() {
                popover.unparent();
            }
        });
    });
    popover.popup();

    // Most widgets don't present popovers they don't know about in their
    // size_allocate, so the menu would keep the size it had before its items
    // were fully set up (eg. accelerator labels). Re-present it for the
    // first few frames.
    let frames = Cell::new(0);
    popover.add_tick_callback(move |popover, _| {
        if popover.parent().is_some() && popover.is_visible() {
            popover.present();
        }
        frames.set(frames.get() + 1);
        if frames.get() < 3 { glib::ControlFlow::Continue } else { glib::ControlFlow::Break }
    });
}

/// A scrolled window with a frame around child
pub fn scrolled_window(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .has_frame(true)
        .child(child)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(format_time_short(3725), "01:02:05");
        assert_eq!(format_ratio(1.236), "1.23");
        assert_eq!(format_percent(5.555), "5.55%");
        assert_eq!(truncd(1.239, 2), 1.23);
        assert!(
            add_links_to_text("see http://a.b/c?d=1 & more")
                .contains("<a href='http://a.b/c?d=1'>")
        );
    }
}
