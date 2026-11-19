//! Formatting values for display: sizes, speeds, times and ratios as
//! Transmission formats them, and links in text.

use gtk::glib;

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
